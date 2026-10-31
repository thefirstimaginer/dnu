#ifndef DREAMCORE_TIME_H
#define DREAMCORE_TIME_H

#include <stddef.h>

typedef long time_t;
typedef long clock_t;

struct timespec {
    time_t tv_sec;
    long tv_nsec;
};

struct tm {
    int tm_sec;
    int tm_min;
    int tm_hour;
    int tm_mday;
    int tm_mon;
    int tm_year;
    int tm_wday;
    int tm_yday;
    int tm_isdst;
};

#define CLOCKS_PER_SEC 1000000L
#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1

time_t time(time_t *result);
clock_t clock(void);
double difftime(time_t end, time_t beginning);
int nanosleep(const struct timespec *request, struct timespec *remaining);
struct tm *gmtime(const time_t *value);
struct tm *localtime(const time_t *value);
struct tm *gmtime_r(const time_t *value, struct tm *result);
struct tm *localtime_r(const time_t *value, struct tm *result);

#endif
