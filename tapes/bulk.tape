Output bulk.gif

Set FontSize 18
Set Width 1280
Set Height 600

Type "cat employees.ndjson"
Sleep 500ms
Enter
Sleep 1s

Type "./escli bulk --refresh wait_for --input employees.ndjson"
Sleep 200ms
Enter
Enter

Sleep 5s