#ifndef DREAMCORE_STDIO_H
#define DREAMCORE_STDIO_H

#include <stddef.h>
#include <stdarg.h>

#define EOF (-1)

typedef struct _IO_FILE FILE;

extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;

size_t fread(void *buffer, size_t size, size_t count, FILE *stream);
size_t fwrite(const void *buffer, size_t size, size_t count, FILE *stream);
FILE *fopen(const char *path, const char *mode);
int fclose(FILE *stream);
FILE *fdopen(int descriptor, const char *mode);
FILE *freopen(const char *path, const char *mode, FILE *stream);
int fileno(FILE *stream);
int fflush(FILE *stream);
int fseek(FILE *stream, long offset, int whence);
long ftell(FILE *stream);
void rewind(FILE *stream);
int ferror(FILE *stream);
int feof(FILE *stream);
void clearerr(FILE *stream);
int fprintf(FILE *stream, const char *format, ...);
int printf(const char *format, ...);
int vprintf(const char *format, va_list arguments);
int puts(const char *text);
int putchar(int character);
int getchar(void);
int fgetc(FILE *stream);
char *fgets(char *buffer, int length, FILE *stream);
int fputc(int character, FILE *stream);
int fputs(const char *text, FILE *stream);
int snprintf(char *buffer, size_t length, const char *format, ...);
int vsnprintf(char *buffer, size_t length, const char *format, va_list arguments);
int vfprintf(FILE *stream, const char *format, va_list arguments);
int sprintf(char *buffer, const char *format, ...);
int vsprintf(char *buffer, const char *format, va_list arguments);

#endif