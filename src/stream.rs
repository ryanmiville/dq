use std::{
    fs::File,
    io::{self, Read, Write},
    os::fd::{AsFd, AsRawFd},
    thread,
};

use anyhow::{Context, Result, ensure};

use crate::plan::Plan;

const MAGIC: &[u8; 4] = b"DQSP";
const VERSION: u32 = 1;
const PREFIX_LEN: usize = 12;
const MAX_HEADER_LEN: usize = 1024 * 1024;

pub fn duplicate_stdin() -> Result<File> {
    let stdin = io::stdin();
    let descriptor = stdin
        .as_fd()
        .try_clone_to_owned()
        .context("failed to duplicate stdin")?;
    Ok(File::from(descriptor))
}

pub fn read_plan_header(mut input: impl Read) -> Result<Plan> {
    let mut prefix = [0_u8; PREFIX_LEN];
    input
        .read_exact(&mut prefix)
        .context("failed to read dq stream header")?;

    ensure!(&prefix[..4] == MAGIC, "invalid dq stream magic");

    let version = u32::from_be_bytes(prefix[4..8].try_into().expect("fixed-size version"));
    ensure!(
        version == VERSION,
        "unsupported dq stream version: {version}"
    );

    let header_len =
        u32::from_be_bytes(prefix[8..12].try_into().expect("fixed-size header length")) as usize;
    ensure!(
        header_len <= MAX_HEADER_LEN,
        "dq stream header exceeds {MAX_HEADER_LEN} bytes"
    );

    let mut header = vec![0_u8; header_len];
    input
        .read_exact(&mut header)
        .context("failed to read dq plan header")?;
    Plan::from_json_slice(&header)
}

pub fn write_plan_header(mut output: impl Write, plan: &Plan) -> Result<()> {
    let header = plan.to_json_vec()?;
    let header_len = u32::try_from(header.len()).context("dq plan header is too large")?;
    ensure!(
        header.len() <= MAX_HEADER_LEN,
        "dq stream header exceeds {MAX_HEADER_LEN} bytes"
    );

    output
        .write_all(MAGIC)
        .and_then(|()| output.write_all(&VERSION.to_be_bytes()))
        .and_then(|()| output.write_all(&header_len.to_be_bytes()))
        .and_then(|()| output.write_all(&header))
        .context("failed to write dq stream header")
}

pub fn prepare_stdin_payload(input: File) -> Result<Option<thread::JoinHandle<()>>> {
    if !input
        .metadata()
        .context("failed to inspect stdin")?
        .file_type()
        .is_file()
    {
        drop(input);
        return Ok(None);
    }

    let (reader, mut writer) = io::pipe().context("failed to create stdin payload pipe")?;
    if unsafe { libc::dup2(reader.as_raw_fd(), libc::STDIN_FILENO) } == -1 {
        return Err(io::Error::last_os_error())
            .context("failed to replace stdin with payload pipe");
    }
    drop(reader);

    Ok(Some(thread::spawn(move || {
        let mut input = input;
        let _ = io::copy(&mut input, &mut writer);
    })))
}

pub fn finish_stdin_payload(handle: Option<thread::JoinHandle<()>>) {
    let Some(handle) = handle else {
        return;
    };

    // The endpoint has finished reading. Closing fd 0 wakes the forwarding
    // thread with EPIPE if DuckDB stopped before consuming the entire payload.
    unsafe {
        libc::close(libc::STDIN_FILENO);
    }
    let _ = handle.join();
}

pub fn write_plan_and_payload(
    plan: &Plan,
    mut input: Option<impl Read>,
    mut output: impl Write,
) -> Result<()> {
    let result = (|| {
        write_plan_header(&mut output, plan)?;
        if let Some(input) = input.as_mut() {
            io::copy(input, &mut output).context("failed to forward input payload")?;
        }
        output.flush().context("failed to flush dq stream")
    })();

    if result.as_ref().is_err_and(is_broken_pipe) {
        Ok(())
    } else {
        result
    }
}

pub fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
            || cause.to_string().contains("Broken pipe")
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Seek};

    use super::*;
    use crate::plan::Plan;

    fn encoded(plan: &Plan, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_plan_header(&mut bytes, plan).unwrap();
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn reads_only_the_header_and_leaves_payload_untouched() {
        let payload = b"\0not utf-8: \xff\n";
        let bytes = encoded(&Plan::from_stream("read_csv('/dev/stdin')"), payload);
        let mut temp = tempfile::tempfile().unwrap();
        temp.write_all(&bytes).unwrap();
        temp.rewind().unwrap();

        let plan = read_plan_header(&mut temp).unwrap();
        let mut remaining = Vec::new();
        temp.read_to_end(&mut remaining).unwrap();

        assert_eq!(plan, Plan::from_stream("read_csv('/dev/stdin')"));
        assert_eq!(remaining, payload);
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut bytes = encoded(&Plan::from_path("input.json"), b"");
        bytes[..4].copy_from_slice(b"NOPE");
        let mut temp = tempfile_from(&bytes);

        let error = read_plan_header(&mut temp).unwrap_err();

        assert!(error.to_string().contains("invalid dq stream magic"));
    }

    #[test]
    fn rejects_unsupported_stream_version() {
        let mut bytes = encoded(&Plan::from_path("input.json"), b"");
        bytes[4..8].copy_from_slice(&2_u32.to_be_bytes());
        let mut temp = tempfile_from(&bytes);

        let error = read_plan_header(&mut temp).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unsupported dq stream version: 2")
        );
    }

    #[test]
    fn rejects_oversized_header_before_allocating_it() {
        let mut bytes = Vec::from(*MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(&((MAX_HEADER_LEN as u32) + 1).to_be_bytes());
        let mut temp = tempfile_from(&bytes);

        let error = read_plan_header(&mut temp).unwrap_err();

        assert!(error.to_string().contains("dq stream header exceeds"));
    }

    #[test]
    fn writes_binary_payload_verbatim_after_header() {
        let plan = Plan::from_stream("read_csv('/dev/stdin')");
        let payload = b"\0raw\xffbytes\n";
        let mut output = Vec::new();

        write_plan_and_payload(&plan, Some(Cursor::new(payload)), &mut output).unwrap();

        let mut cursor = Cursor::new(output);
        assert_eq!(read_plan_header(&mut cursor).unwrap(), plan);
        let mut remaining = Vec::new();
        cursor.read_to_end(&mut remaining).unwrap();
        assert_eq!(remaining, payload);
    }

    fn tempfile_from(bytes: &[u8]) -> File {
        let mut temp = tempfile::tempfile().unwrap();
        temp.write_all(bytes).unwrap();
        temp.rewind().unwrap();
        temp
    }
}
