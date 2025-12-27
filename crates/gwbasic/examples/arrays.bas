10 REM Array Operations Test
20 PRINT "=== Array Operations ==="
30 PRINT ""
40 REM Dimension an array
50 DIM NUMBERS(10)
60 PRINT "Filling array with squares:"
70 FOR I = 1 TO 10
80   LET NUMBERS(I) = I * I
90   PRINT "NUMBERS("; I; ") ="; NUMBERS(I)
100 NEXT I
110 PRINT ""
120 REM Calculate sum
130 LET SUM = 0
140 FOR I = 1 TO 10
150   LET SUM = SUM + NUMBERS(I)
160 NEXT I
170 PRINT "Sum of all elements:"; SUM
180 PRINT "Average:"; SUM / 10
190 PRINT ""
200 REM Find maximum
210 LET MAX = NUMBERS(1)
220 FOR I = 2 TO 10
230   IF NUMBERS(I) > MAX THEN LET MAX = NUMBERS(I)
240 NEXT I
250 PRINT "Maximum value:"; MAX
260 END
