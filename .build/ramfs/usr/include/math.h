#ifndef DREAMCORE_MATH_H
#define DREAMCORE_MATH_H

#define HUGE_VAL (__builtin_huge_val())
#define INFINITY (__builtin_inff())
#define NAN (__builtin_nanf(""))
#define isfinite(value) __builtin_isfinite(value)
#define isnan(value) __builtin_isnan(value)
#define isinf(value) __builtin_isinf(value)
#define signbit(value) __builtin_signbit(value)

double fabs(double value);
double floor(double value);
double ceil(double value);
double trunc(double value);
double sqrt(double value);
double pow(double base, double exponent);
double ldexp(double value, int exponent);
long double ldexpl(long double value, int exponent);
double frexp(double value, int *exponent);

#endif
