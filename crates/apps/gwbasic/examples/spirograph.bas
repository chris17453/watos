10 REM Spirograph / Geometric Pattern
20 PRINT "=== Spirograph Pattern ==="
30 SCREEN 1
40 CLS
50 COLOR 0, 0
60 REM Center of screen
70 LET CX = 160
80 LET CY = 100
90 REM Draw spirograph pattern
100 LET R1 = 80: REM Outer radius
110 LET R2 = 40: REM Inner radius
120 LET D = 30: REM Distance from inner circle center
130 LET STEPS = 360
140 LET LASTX = 0
150 LET LASTY = 0
160 FOR T = 0 TO STEPS
170   LET ANGLE = T * 6.28318 / STEPS
180   LET X = CX + (R1 - R2) * COS(ANGLE) + D * COS((R1 - R2) * ANGLE / R2)
190   LET Y = CY + (R1 - R2) * SIN(ANGLE) - D * SIN((R1 - R2) * ANGLE / R2)
200   IF T > 0 THEN LINE (LASTX, LASTY)-(X, Y), (T MOD 3) + 1
210   LET LASTX = X
220   LET LASTY = Y
230 NEXT T
240 REM Title
250 LOCATE 1, 1
260 PRINT "SPIROGRAPH"
270 LOCATE 23, 1
280 PRINT "Press any key..."
310 END
