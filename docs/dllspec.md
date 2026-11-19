# DNU Linkable Library (DLL)

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** Kernel DNU e userspace do ImagineOS

---

## 1. Introdução

O **DLL** (DNU Linkable Library) é o formato de biblioteca compartilhada do DNU. Bibliotecas DLL permitem que código seja compilado uma vez e reutilizado por múltiplos programas de userspace, reduzindo o uso de memória e permitindo atualizações independentes de componentes de sistema.

Ao contrário dos executáveis DEF, que são programas independentes com ponto de entrada próprio, uma DLL exporta símbolos (funções e dados) que podem ser resolvidos e vinculados por outros programas em tempo de carga ou execução.

---

## 2. Formato de Arquivo `.dll`

Uma DLL é um arquivo no formato **DEF** (veja `defspec.md`) com uma tabela de exportações embutida na seção de metadados de segurança.

### 2.1 Identificação

| Campo | Valor |
|-------|-------|
| `magic` | `"DNUF"` (0x44 0x4E 0x55 0x46) |
| `type` | Reservado para biblioteca (a ser definido no `DefType`) |
| `security_offset` | Aponta para a seção de metadados que contém a tabela de exportações |

### 2.2 Tabela de Exportações

Localizada dentro da seção de metadados de segurança (`security_offset` / `security_size`). O formato é TLV (Type-Length-Value), com os seguintes tipos:

| Type | Nome | Value | Descrição |
|------|------|-------|-----------|
| 0x10 | `EXPORT_FUNC` | UTF-8 + endereço | Nome da função exportada e seu endereço virtual |
| 0x11 | `EXPORT_DATA` | UTF-8 + endereço | Nome de dado exportado e seu endereço virtual |
| 0x12 | `DLL_NAME` | UTF-8 | Nome canônico da biblioteca (ex: `libc.dnu`) |
| 0x13 | `DLL_VERSION` | u32 | Versão da ABI da biblioteca |
| 0x14 | `DLL_DEPENDS` | UTF-8 | Lista de DLLs dependentes, separadas por `;` |

Cada entrada `EXPORT_FUNC` / `EXPORT_DATA` tem o seguinte layout:

```
┌──────────────────┬──────────────────┬──────────────┐
│ Name Length (2B) │ Name (UTF-8)     │ Address (8B) │
└──────────────────┴──────────────────┴──────────────┘
```

O nome é uma string UTF-8 terminada por null (o length inclui o null). O address é o endereço virtual onde o símbolo estará após o carregamento da DLL.

---

## 3. Carregamento de DLL

Quando um programa DEF importa símbolos de uma DLL, o loader do kernel:

1. Localiza o arquivo `.dll` no caminho especificado (padrão: `/Libraries/` ou caminho absoluto).
2. Carrega o arquivo como DEF e valida o magic e a ABI.
3. Resolve cada símbolo importado contra a tabela de exportações da DLL.
4. Mapeia as páginas da DLL no address space do processo (com permissões adequadas: READ para código, READ|WRITE para dados).
5. Realoca endereços se necessário (relocação baseada em `image_base`).
6. Retorna os endereços resolvidos para o programa chamador.

Se qualquer símbolo não puder ser resolvido, o carregamento falha com `ERR_SYMBOL_UNDEFINED`.

---

## 4. Nomeação e Locais

| Caminho | Uso |
|---------|-----|
| `/Libraries/System/` | Bibliotecas de sistema (libc, libm, libpthread, etc.) |
| `/Libraries/<app>/` | Bibliotecas privadas de uma aplicação |
| `/System/Users/<usuario>/local#Libraries/` | Bibliotecas de usuário |

Convenção de nomeação: `lib<nome>.dnu` (ex: `libc.dnu`, `libm.dnu`, `libdl.dnu`).

---

## 5. Versionamento de ABI

O campo `DLL_VERSION` (u32) codifica a versão da ABI da biblioteca no formato `major<<24 | minor<<16 | patch<<8 | build`. Programas que dependem de uma DLL devem declarar a versão mínima esperada; se a DLL instalada for incompatível, o loader recusa a execução.

---

## 6. Exemplo de Uso

```bash
# Um programa userspace importa printf de libc.dnu
# No momento do carregamento, o kernel:
# 1. Carrega /Libraries/System/libc.dnu
# 2. Resolve printf na tabela de exportações
# 3. Mapeia a DLL no address space do processo
# 4. Ajusta a tabela de importação do programa com o endereço resolvido
```

---

## Referências

- [Formato DEF](./defspec.md) — formato base dos arquivos `.dll`
- [ABI e Syscalls](./abispec.md) — convenção de chamada e números de syscall
