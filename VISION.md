# dq vision

This will be a CLI toolkit to query and manipulate data using duckdb. It is inspired by the DX of nushell's data pipelines and Google's pipe query syntax. 

## Example Usage

```bash
echo '[{"name":"Ada","age":37},{"name":"Linus","age":54}]' |
dq from json |
dq where "age > 40" |
dq select "name"
# => [{"name":"Linus"}]
```

## Tech choices
- duckdb for the query engine
- clap for CLI

## Implementation ideas

Naively, the example above could be replaced with:

```bash
echo '[{"name":"Ada","age":37},{"name":"Linus","age":54}]' |
duckdb -json -c "SELECT * FROM read_json_auto('/dev/stdin') WHERE age > 40" |
duckdb -json -c "SELECT name FROM read_json_auto('/dev/stdin')"
```

But this assumes json. We can start our implementation targeting json, but eventually we want to support all the types duckdb supports (CSV, parquet, etc.)

We will need to think carefully about how we want to represent the table after initial parsing. nushell converts data to a standard representation, and then the user can finish with `... | to json`, `... | to csv`, etc. That seems like a good approach, but I haven't determined how that should look for our application.

## commands

- `dq from [data type]`
- `dq select [columns]`
- `dq where [clause]`
- `dq order by [columns (optional direction)]`
- `dq limit [count]`

## features/commands that will exist, but I have not made final decisions on yet
- aggregations
- output format

## important decisions that need to be made
- intermediate representation
  - naively, we can just always convert to json
    - i don't love that it would always output json though. that's kind of weird if you were querying csv and it output json.
  - idk how we can make the next command infer the type, but i think it would be nice if it outputs the same as the input (csv in -> csv out, etc.)
