10 REM Number Guessing Game
20 PRINT "=== Number Guessing Game ==="
30 PRINT ""
40 PRINT "I'm thinking of a number between 1 and 100"
50 PRINT ""
60 REM Generate random number
70 RANDOMIZE TIMER
80 LET TARGET = INT(RND(1) * 100) + 1
90 LET TRIES = 0
100 REM Main game loop
110 INPUT "Enter your guess"; GUESS
120 LET TRIES = TRIES + 1
130 IF GUESS = TARGET THEN GOTO 200
140 IF GUESS < TARGET THEN PRINT "Too low! Try again."
150 IF GUESS > TARGET THEN PRINT "Too high! Try again."
160 GOTO 110
200 REM Win condition
210 PRINT ""
220 PRINT "Congratulations! You guessed it!"
230 PRINT "The number was:"; TARGET
240 PRINT "It took you"; TRIES; "tries."
250 END
