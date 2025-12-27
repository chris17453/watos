10 REM Flower Drawing Program
20 PRINT "=== Drawing Flowers ==="
30 SCREEN 1
40 CLS
50 COLOR 0, 1
60 REM Draw a simple flower using circles
70 REM Center of screen
80 LET CX = 160
90 LET CY = 100
100 REM Draw flower petals (6 petals in a circle)
110 FOR I = 0 TO 5
120   LET ANGLE = I * 60 * 3.14159 / 180
130   LET PX = CX + 30 * COS(ANGLE)
140   LET PY = CY + 30 * SIN(ANGLE)
150   CIRCLE (PX, PY), 15, 2
160 NEXT I
170 REM Draw center of flower
180 CIRCLE (CX, CY), 10, 3
190 PAINT (CX, CY), 3, 3
200 REM Draw stem
210 LINE (CX, CY + 10)-(CX, CY + 60), 1
220 REM Draw leaves
230 CIRCLE (CX - 10, CY + 40), 8, 1, 0, 3.14159
240 CIRCLE (CX + 10, CY + 50), 8, 1, 0, 3.14159
250 REM Title
260 LOCATE 1, 1
270 PRINT "FLOWER GARDEN"
280 REM Wait for keypress
290 LOCATE 23, 1
300 PRINT "Press any key to continue..."
340 CLS
350 END
