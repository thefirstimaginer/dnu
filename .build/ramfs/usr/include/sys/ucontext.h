#ifndef DREAMCORE_SYS_UCONTEXT_H
#define DREAMCORE_SYS_UCONTEXT_H

typedef long greg_t;
typedef greg_t gregset_t[23];

typedef struct {
    gregset_t gregs;
} mcontext_t;

typedef struct {
    mcontext_t uc_mcontext;
} ucontext_t;

#define REG_RBP 10
#define REG_RIP 16

#endif
