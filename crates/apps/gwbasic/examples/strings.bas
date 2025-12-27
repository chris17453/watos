10 REM String Operations Test
20 PRINT "=== String Operations ==="
30 PRINT ""
40 LET NAME$ = "Alice"
50 LET GREETING$ = "Hello"
60 PRINT GREETING$; ", "; NAME$; "!"
70 PRINT ""
80 REM String concatenation
90 LET MESSAGE$ = "Welcome to GW-BASIC"
100 PRINT MESSAGE$
110 PRINT ""
120 REM String functions
130 LET TEXT$ = "BASIC"
140 PRINT "Original string: "; TEXT$
150 PRINT "Length: LEN("; TEXT$; ") ="; LEN(TEXT$)
160 PRINT "LEFT$("; TEXT$; ", 3) ="; LEFT$(TEXT$, 3)
170 PRINT "RIGHT$("; TEXT$; ", 2) ="; RIGHT$(TEXT$, 2)
180 PRINT "MID$("; TEXT$; ", 2, 3) ="; MID$(TEXT$, 2, 3)
190 END
