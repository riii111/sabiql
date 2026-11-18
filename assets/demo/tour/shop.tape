Set Shell bash
Set Width 1750
Set Height 1090
Set FontFamily "DroidSansM Nerd Font Mono"
Set FontSize 16
Set LineHeight 1.0
Set Padding 12
Set Margin 22
Set MarginFill "#808080"
Set BorderRadius 11
Set WindowBar Colorful
Set WindowBarSize 33
Set Theme "Catppuccin Mocha"
Set Framerate 18
Set TypingSpeed 95ms

Hide
Type@1ms `stty size > "$SABIQL_DEMO_DIR/cells"; cd "$SABIQL_DEMO_DIR/northline" && exec env -u NO_COLOR HOME="$SABIQL_DEMO_DIR/profile" PGSERVICEFILE="$SABIQL_DEMO_DIR/service.conf" PATH="$SABIQL_DEMO_DIR/viewer:$PATH" TERM=xterm-256color COLORTERM=truecolor "$SABIQL_DEMO_BIN"`
Enter
Wait+Screen@15s /Select Connection/
Enter
Wait+Screen@15s /public.collections/
Ctrl+P
Type@20ms "collections"
Enter
Wait+Screen@10s /Everyday goods/
Type "2"
Tab
Sleep 400ms
Type "1"
Show

Sleep 1350ms
Ctrl+P
Sleep 480ms
Type@155ms "prod"
Sleep 800ms
Enter
Wait+Screen@8s /Coffe Mug/
Type "2"
Sleep 1300ms
Tab
Sleep 750ms
Tab
Sleep 1350ms
Tab
Wait+Screen@5s /published_catalog/
Sleep 2100ms

Type "3"
Sleep 350ms
Enter
Type "l"
Sleep 330ms
Type "j"
Sleep 380ms
Type "j"
Sleep 900ms
Type "i"
Sleep 350ms
Left@135ms 4
Type "e"
Sleep 650ms
Type@165ms ":w"
Enter
Wait+Screen@5s /Confirm UPDATE/
Sleep 2550ms
Enter
Wait+Screen@8s /Updated 1 row/
Sleep 950ms

Type "s"
Sleep 500ms
Type "i"
Sleep 230ms
Type@59ms "SELECT "
Sleep 255ms
Type@89ms "name,"
Sleep 410ms
Type@75ms " stock"
Sleep 190ms
Type@54ms " FROM "
Type@126ms "pr"
Type@68ms "od"
Sleep 1060ms
Tab
Sleep 490ms
Type@63ms " WHERE "
Sleep 280ms
Type@82ms "stock"
Type@140ms " < "
Sleep 390ms
Type@74ms "20;"
Sleep 820ms
Alt+Enter
Wait+Screen@8s /3 rows/
Sleep 580ms
Escape
Wait+Screen@8s /Result Query/
Sleep 2050ms

Type "e"
Wait+Screen@10s /ER Diagram/
Sleep 450ms
Type@150ms "prod"
Sleep 450ms
Space
Sleep 1500ms
Enter
Wait+Screen@16s /Opened/
Sleep 400ms
