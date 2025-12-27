10 REM Mandelbrot Set Renderer
20 PRINT "=== Mandelbrot Set ==="
30 PRINT "Rendering... Please wait..."
40 SCREEN 1
50 CLS
60 COLOR 0, 0
70 REM Screen dimensions
80 LET SW = 320
90 LET SH = 200
100 REM Mandelbrot parameters
110 LET XMIN = -2.5
120 LET XMAX = 1.5
130 LET YMIN = -1.5
140 LET YMAX = 1.5
150 LET MAXITER = 16
160 REM Calculate each pixel
170 FOR SY = 0 TO SH - 1
180   FOR SX = 0 TO SW - 1
190     REM Map screen coords to complex plane
200     LET X0 = XMIN + (XMAX - XMIN) * SX / SW
210     LET Y0 = YMIN + (YMAX - YMIN) * SY / SH
220     REM Initialize iteration variables
230     LET X = 0
240     LET Y = 0
250     LET ITER = 0
260     REM Iterate the Mandelbrot formula
270     LET X2 = 0
280     LET Y2 = 0
290     IF X2 + Y2 > 4 THEN GOTO 370
300     IF ITER >= MAXITER THEN GOTO 370
310     LET Y = 2 * X * Y + Y0
320     LET X = X2 - Y2 + X0
330     LET X2 = X * X
340     LET Y2 = Y * Y
350     LET ITER = ITER + 1
360     GOTO 290
370     REM Color based on iteration count (use all 16 colors)
380     LET C = ITER
390     IF ITER >= MAXITER THEN LET C = 0
400     PSET (SX, SY), C
410   NEXT SX
420   REM Show progress
430   IF SY MOD 10 = 0 THEN LOCATE 1, 1: PRINT "Progress:"; INT(SY * 100 / SH); "%  "
440 NEXT SY
450 REM Finished
460 LOCATE 1, 1
470 PRINT "Mandelbrot Set - Complete!   "
480 LOCATE 23, 1
490 PRINT "Press any key to exit..."
540 END
