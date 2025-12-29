10 REM Mandelbrot Set - SVGA High Resolution
20 PRINT "=== Mandelbrot Set (SVGA 800x600) ==="
30 PRINT "Rendering... This will take a moment..."
40 SCREEN 4
50 CLS
60 REM Screen dimensions for SVGA
70 LET SW = 800
80 LET SH = 600
90 REM Mandelbrot parameters (zoom to interesting area)
100 LET XMIN = -0.7
110 LET XMAX = -0.4
120 LET YMIN = -0.2
130 LET YMAX = 0.1
140 LET MAXITER = 64
150 REM Calculate each pixel (sample every 2 pixels for speed)
160 FOR SY = 0 TO SH - 1 STEP 2
170   FOR SX = 0 TO SW - 1 STEP 2
180     REM Map screen coords to complex plane
190     LET X0 = XMIN + (XMAX - XMIN) * SX / SW
200     LET Y0 = YMIN + (YMAX - YMIN) * SY / SH
210     REM Initialize iteration variables
220     LET X = 0
230     LET Y = 0
240     LET ITER = 0
250     LET X2 = 0
260     LET Y2 = 0
270     REM Iterate the Mandelbrot formula
280     IF X2 + Y2 > 4 THEN GOTO 330
290     IF ITER >= MAXITER THEN GOTO 330
300     LET Y = 2 * X * Y + Y0
310     LET X = X2 - Y2 + X0
320     LET X2 = X * X: LET Y2 = Y * Y: LET ITER = ITER + 1: GOTO 280
330     REM Color based on iteration count (map to 16 colors)
340     LET C = INT(ITER * 15 / MAXITER)
350     IF ITER >= MAXITER THEN LET C = 0
360     PSET (SX, SY), C
370     REM Also fill neighbor pixel for speed
380     IF SX + 1 < SW THEN PSET (SX + 1, SY), C
390     IF SY + 1 < SH THEN PSET (SX, SY + 1), C
400     IF SX + 1 < SW AND SY + 1 < SH THEN PSET (SX + 1, SY + 1), C
410   NEXT SX
420   REM Show progress every 20 lines
430   IF SY MOD 20 = 0 THEN LOCATE 1, 1: PRINT "Progress:"; INT(SY * 100 / SH); "%  "
440 NEXT SY
450 REM Finished
460 LOCATE 1, 1
470 PRINT "Mandelbrot Set - Complete! (800x600 SVGA)   "
480 LOCATE 2, 1
490 PRINT "Press any key to exit..."
500 END
