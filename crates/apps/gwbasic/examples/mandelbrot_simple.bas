10 REM Mandelbrot Set - Simplified
20 PRINT "=== Mandelbrot Set ==="
30 PRINT "Rendering (this takes a moment)..."
40 SCREEN 1
50 CLS
60 LET XMIN = -2.5
70 LET XMAX = 1.0
80 LET YMIN = -1.0
90 LET YMAX = 1.0
100 LET MAXITER = 16
110 REM Calculate each pixel (reduced resolution for speed)
120 FOR SY = 0 TO 199 STEP 4
130   FOR SX = 0 TO 319 STEP 2
140     LET X0 = XMIN + (XMAX - XMIN) * SX / 320
150     LET Y0 = YMIN + (YMAX - YMIN) * SY / 200
160     LET X = 0
170     LET Y = 0
180     LET ITER = 0
190     LET X2 = 0
200     LET Y2 = 0
210     IF X2 + Y2 > 4 THEN GOTO 260
220     IF ITER >= MAXITER THEN GOTO 260
230     LET Y = 2 * X * Y + Y0
240     LET X = X2 - Y2 + X0
250     LET X2 = X * X: LET Y2 = Y * Y: LET ITER = ITER + 1: GOTO 210
260     IF ITER >= MAXITER THEN PSET (SX, SY), 1
270   NEXT SX
280 NEXT SY
290 PRINT "MANDELBROT COMPLETE!"
300 END
