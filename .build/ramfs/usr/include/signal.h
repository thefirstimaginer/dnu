#ifndef DREAMCORE_SIGNAL_H
#define DREAMCORE_SIGNAL_H

typedef void (*sighandler_t)(int);
typedef unsigned long sigset_t;

typedef struct {
    int si_signo;
    int si_errno;
    int si_code;
} siginfo_t;

struct sigaction {
    union {
        sighandler_t sa_handler;
        void (*sa_sigaction)(int, siginfo_t *, void *);
    };
    unsigned long sa_flags;
    void (*sa_restorer)(void);
    sigset_t sa_mask;
};

#define SIGHUP 1
#define SIGINT 2
#define SIGABRT 6
#define SIGBUS 7
#define SIGFPE 8
#define SIGILL 4
#define SIGKILL 9
#define SIGSEGV 11
#define SIGTERM 15
#define SIGCONT 18
#define SIGSTOP 19
#define SIG_DFL ((sighandler_t)0)
#define SIG_IGN ((sighandler_t)1)
#define SIG_ERR ((sighandler_t)-1)
#define SA_SIGINFO 0x00000004
#define SIG_UNBLOCK 1
#define FPE_INTDIV 1
#define FPE_FLTDIV 3

int sigaction(int signal_number, const struct sigaction *action, struct sigaction *old_action);
sighandler_t signal(int signal_number, sighandler_t handler);
int sigemptyset(sigset_t *set);
int sigaddset(sigset_t *set, int signal_number);
int sigprocmask(int how, const sigset_t *set, sigset_t *old_set);

#endif
