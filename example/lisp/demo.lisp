; som run example/lisp < example/lisp/demo.lisp

(+ 1 2 3)
(* (- 10 4) (/ 21 3))

; Recursion
(define (fact n)
  (if (<= n 1) 1 (* n (fact (- n 1)))))
(fact 20)

(define (fib n)
  (cond ((< n 2) n)
        (else (+ (fib (- n 1)) (fib (- n 2))))))
(fib 20)

; Lists and higher-order functions
(define (map f xs)
  (if (null? xs) '() (cons (f (car xs)) (map f (cdr xs)))))
(define (filter keep xs)
  (cond ((null? xs) '())
        ((keep (car xs)) (cons (car xs) (filter keep (cdr xs))))
        (else (filter keep (cdr xs)))))
(define (fold f acc xs)
  (if (null? xs) acc (fold f (f acc (car xs)) (cdr xs))))
(define (range a b)
  (if (>= a b) '() (cons a (range (+ a 1) b))))

(map (lambda (x) (* x x)) (range 1 11))
(filter (lambda (x) (= (mod x 3) 0)) (range 1 20))
(fold + 0 (range 1 101))

; Closures that keep state
(define (make-counter)
  (let ((n 0))
    (lambda () (set! n (+ n 1)) n)))
(define tick (make-counter))
(tick)
(tick)
(tick)

; Tail calls run in constant stack
(define (count-down n) (if (= n 0) 'done (count-down (- n 1))))
(count-down 1000000)

; Variadic functions, quoting, strings
(define (count first . rest) (list first (length rest)))
(count 'a 'b 'c 'd)
((lambda args (length args)) 1 2 3 4)
'(1 . 2)
'(a (b c) "d")
(display "hello from lisp")
(newline)

(car '())
(undefined-thing 1)
(fact 1 2)
(+ 1 "two")
(* 6 7)
