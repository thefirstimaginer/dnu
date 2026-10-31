#ifndef DREAMCORE_SYS_TIME_H
#define DREAMCORE_SYS_TIME_H

struct timeval {
    long tv_sec;
    long tv_usec;
};

int gettimeofday(struct timeval *value, void *timezone);

#endif
