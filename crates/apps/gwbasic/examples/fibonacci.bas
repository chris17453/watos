10 REM Fibonacci Sequence Generator
20 PRINT "=== Fibonacci Sequence ==="
30 PRINT ""
40 INPUT "How many Fibonacci numbers"; N
50 PRINT ""
60 IF N < 1 THEN PRINT "Please enter a positive number": END
70 REM Initialize first two numbers
80 LET A = 0
90 LET B = 1
100 PRINT "Fibonacci Sequence:"
110 PRINT 1; ": "; A
120 IF N = 1 THEN END
130 PRINT 2; ": "; B
140 REM Generate remaining numbers
150 FOR I = 3 TO N
160   LET C = A + B
170   PRINT I; ": "; C
180   LET A = B
190   LET B = C
200 NEXT I
210 PRINT ""
220 PRINT "Final Fibonacci number:"; B
230 END
