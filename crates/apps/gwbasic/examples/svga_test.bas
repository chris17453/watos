10 REM SVGA Graphics Test
20 PRINT "=== SVGA Graphics Test ==="
30 PRINT "Testing 800x600 SVGA mode..."
40 PRINT ""
50 PRINT "Switching to SCREEN 4 (800x600 SVGA)..."
60 SCREEN 4
70 CLS
80 REM Draw test pattern
90 PRINT "Drawing test pattern..."
100 REM Draw colored rectangles across the screen
110 FOR I = 0 TO 15
120   LET X = I * 50
130   LET Y = 100
140   LET W = 40
150   LET H = 100
160   LET C = I
170   REM Draw filled rectangle
180   FOR DY = 0 TO H
190     LINE (X, Y + DY)-(X + W, Y + DY), C
200   NEXT DY
210 NEXT I
220 REM Draw some circles
230 FOR I = 1 TO 5
240   LET X = 100 + I * 100
250   LET Y = 400
260   LET R = 30 + I * 5
270   LET C = I + 8
280   CIRCLE (X, Y), R, C
290 NEXT I
300 REM Draw diagonal lines
310 FOR I = 0 TO 10
320   LET X1 = I * 80
330   LET Y1 = 0
340   LET X2 = 800
350   LET Y2 = I * 60
360   LET C = I
370   LINE (X1, Y1)-(X2, Y2), C
380 NEXT I
390 REM Add text at top
400 LOCATE 1, 1
410 PRINT "SVGA 800x600 Test - Colors and Shapes"
420 LOCATE 2, 1
430 PRINT "Press any key to continue..."
440 REM Wait for key
450 END
