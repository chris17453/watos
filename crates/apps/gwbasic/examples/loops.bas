10 REM Loop Examples
20 PRINT "=== FOR/NEXT Loop Test ==="
30 PRINT ""
40 PRINT "Counting 1 to 10:"
50 FOR I = 1 TO 10
60   PRINT "Number:"; I
70 NEXT I
80 PRINT ""
90 PRINT "Counting by 2s (2 to 20):"
100 FOR I = 2 TO 20 STEP 2
110   PRINT I;
120 NEXT I
130 PRINT ""
140 PRINT ""
150 PRINT "Counting down (10 to 1):"
160 FOR I = 10 TO 1 STEP -1
170   PRINT I;
180 NEXT I
190 PRINT ""
200 PRINT ""
210 PRINT "Nested Loops (Multiplication Table 3x3):"
220 FOR I = 1 TO 3
230   FOR J = 1 TO 3
240     PRINT I; "x"; J; "="; I * J; " ";
250   NEXT J
260   PRINT ""
270 NEXT I
280 END
