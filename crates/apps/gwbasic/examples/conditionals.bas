10 REM Conditional Statements Test
20 PRINT "=== IF/THEN/ELSE Test ==="
30 PRINT ""
40 LET X = 15
50 PRINT "X ="; X
60 PRINT ""
70 IF X > 10 THEN PRINT "X is greater than 10"
80 IF X < 10 THEN PRINT "X is less than 10"
90 IF X = 15 THEN PRINT "X equals 15"
100 IF X <> 20 THEN PRINT "X does not equal 20"
110 PRINT ""
120 REM Test with ELSE
130 IF X > 20 THEN PRINT "X > 20" ELSE PRINT "X <= 20"
140 PRINT ""
150 REM Multiple conditions
160 LET A = 5
170 LET B = 10
180 PRINT "A ="; A; ", B ="; B
190 IF A < B THEN PRINT "A is less than B"
200 IF A <= B THEN PRINT "A is less than or equal to B"
210 IF B > A THEN PRINT "B is greater than A"
220 IF B >= A THEN PRINT "B is greater than or equal to A"
230 END
