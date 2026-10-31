#ifndef DREAMCORE_SYS_STAT_H
#define DREAMCORE_SYS_STAT_H

#include <stddef.h>

#define S_IFMT  00170000
#define S_IFSOCK 0140000
#define S_IFLNK  0120000
#define S_IFREG  0100000
#define S_IFBLK  0060000
#define S_IFDIR  0040000
#define S_IFCHR  0020000
#define S_IFIFO  0010000

#define S_ISDIR(mode) (((mode) & S_IFMT) == S_IFDIR)
#define S_ISREG(mode) (((mode) & S_IFMT) == S_IFREG)
#define S_ISLNK(mode) (((mode) & S_IFMT) == S_IFLNK)

struct stat {
    unsigned long st_dev;
    unsigned long st_ino;
    unsigned long st_nlink;
    unsigned int st_mode;
    unsigned int st_uid;
    unsigned int st_gid;
    unsigned int __pad;
    long long st_size;
    long st_blksize;
    long long st_blocks;
    long st_atime;
    long st_mtime;
    long st_ctime;
};

int stat(const char *path, struct stat *buffer);
int fstat(int descriptor, struct stat *buffer);

#endif
