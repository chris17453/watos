10 REM Subroutine Test (GOSUB/RETURN)
20 PRINT "=== Subroutine Test ==="
30 PRINT ""
40 PRINT "Main program starting..."
50 GOSUB 1000
60 PRINT ""
70 LET X = 5
80 LET Y = 3
90 PRINT "Calling multiply subroutine with X="; X; "and Y="; Y
100 GOSUB 2000
110 PRINT "Result:"; RESULT
120 PRINT ""
130 PRINT "Main program ending."
140 END
1000 REM Subroutine: Print Header
1010 PRINT "  +------------------------+"
1020 PRINT "  |  Subroutine Example   |"
1030 PRINT "  +------------------------+"
1040 RETURN
2000 REM Subroutine: Multiply two numbers
2010 LET RESULT = X * Y
2020 RETURN
