Output esql.gif

Set FontSize 18
Set Width 1280
Set Height 600

Type `./escli esql query --format csv <<< '{"query": "FROM employees | LIMIT 1"}'`
Sleep 200ms
Enter

Type `./escli esql query --format txt <<< '{"query": "FROM employees | KEEP first_name, last_name | LIMIT 5"}'`
Sleep 200ms
Enter

Sleep 5s

