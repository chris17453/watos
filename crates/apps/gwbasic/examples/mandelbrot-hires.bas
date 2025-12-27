10 REM Mandelbrot Set - High Resolution
20 PRINT "=== Mandelbrot Set (High-Res) ==="
30 PRINT "This may take a while..."
40 INPUT "Max iterations (16-100)"; MAXITER
50 IF MAXITER < 16 THEN MAXITER = 16
60 IF MAXITER > 100 THEN MAXITER = 100
70 SCREEN 2
80 CLS
90 REM Screen dimensions for high-res mode
100 LET SW = 640
110 LET SH = 200
120 REM Mandelbrot parameters
130 LET XMIN = -2.5
140 LET XMAX = 1.0
150 LET YMIN = -1.0
160 LET YMAX = 1.0
170 PRINT "Rendering..."
180 REM Calculate each pixel
190 FOR SY = 0 TO SH - 1
200   FOR SX = 0 TO SW - 1
210     REM Map screen coords to complex plane
220     LET X0 = XMIN + (XMAX - XMIN) * SX / SW
230     LET Y0 = YMIN + (YMAX - YMIN) * SY / SH
240     REM Initialize iteration variables
250     LET X = 0
260     LET Y = 0
270     LET ITER = 0
280     REM Iterate
290     LET X2 = X * X
300     LET Y2 = Y * Y
310     IF X2 + Y2 > 4 THEN GOTO 370
320     IF ITER >= MAXITER THEN GOTO 370
330     LET YTEMP = 2 * X * Y + Y0
340     LET X = X2 - Y2 + X0
350     LET Y = YTEMP
360     LET ITER = ITER + 1: GOTO 290
370     REM Draw pixel if in set
380     IF ITER >= MAXITER THEN PSET (SX, SY), 1
390   NEXT SX
400   REM Progress indicator
410   IF SY MOD 10 = 0 THEN LOCATE 1, 1: PRINT INT(SY * 100 / SH); "%"
420 NEXT SY
430 LOCATE 1, 1
440 PRINT "Done!    "
450 LOCATE 23, 1
460 PRINT "Press any key..."
470 A$ = INPUT$(1)
480 SCREEN 0: WIDTH 80: CLS
490 END
