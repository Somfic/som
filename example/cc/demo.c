// som run example/cc < example/cc/demo.c > demo.s && cc demo.s -o demo && ./demo
#include <stdio.h>

int printf(char fmt, ...);
int putchar(int c);

int fib(int n) {
    if (n < 2) return n;
    return fib(n - 1) + fib(n - 2);
}

int gcd(int a, int b) {
    while (b != 0) {
        int t = b;
        b = a % b;
        a = t;
    }
    return a;
}

int is_prime(int n) {
    if (n < 2) return 0;
    for (int d = 2; d * d <= n; d++) {
        if (n % d == 0) return 0;
    }
    return 1;
}

/* Collatz: steps to reach 1. */
int collatz(long n) {
    int steps = 0;
    while (n != 1) {
        n = n % 2 == 0 ? n / 2 : 3 * n + 1;
        steps += 1;
    }
    return steps;
}

int sum_odd_below(int limit) {
    int total = 0;
    for (int i = 0; ; i++) {
        if (i >= limit) break;
        if (i % 2 == 0) continue;
        total += i;
    }
    return total;
}

int max8(int a, int b, int c, int d, int e, int f, int g, int h) {
    int m = a;
    if (b > m) m = b; if (c > m) m = c; if (d > m) m = d;
    if (e > m) m = e; if (f > m) m = f; if (g > m) m = g;
    if (h > m) m = h;
    return m;
}

void stars(int n) {
    for (int i = 0; i < n; i++) putchar('*');
    putchar('\n');
}

int main() {
    printf("fib(25) = %ld\n", fib(25));
    printf("gcd(1071, 462) = %ld\n", gcd(1071, 462));

    printf("primes:");
    for (int n = 2; n < 50; n++)
        if (is_prime(n)) printf(" %ld", n);
    printf("\n");

    printf("collatz(27) = %ld steps\n", collatz(27));
    printf("sum of odd numbers below 100 = %ld\n", sum_odd_below(100));
    printf("max8 = %ld\n", max8(3, 9, -2, 7, 12, 5, 11, 0));

    int x = 5;
    int y = x++ + ++x;          // 5 + 7
    printf("x = %ld, y = %ld\n", x, y);
    printf("logic: %ld %ld %ld\n", 1 && 0, 0 || 3, !(2 > 1));
    printf("big: %ld\n", 3000000000 * 3);
    printf("char: %c%c, negative: %ld\n", 'o', 'k', -17 / 3);

    {
        int x = 100;            // shadows the outer `x`
        printf("inner x = %ld\n", x);
    }
    printf("outer x = %ld\n", x);

    for (int i = 1; i <= 5; i++) stars(i);
    return 0;
}
