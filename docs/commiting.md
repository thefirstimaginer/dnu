# Convenção de Commits — Kernel DNU

Este documento define o padrão de mensagens de commit para o repositório do
kernel DNU (Dreamcore is Not Unix). É uma adaptação da especificação
[Conventional Commits](https://www.conventionalcommits.org/) ajustada à
estrutura e ao idioma do projeto.

> **Nota:** O idioma padrão das mensagens de commit é **português do Brasil**,
> já que a maior parte do histórico e da documentação do projeto usa esse idioma.

---

## Formato da mensagem

```
<tipo>(<escopo opcional>): <descrição>

<corpo opcional>

<rodapé opcional>
```

Onde:

- **`<tipo>`**: obrigatório. Indica a natureza da alteração.
- **`<escopo>`**: opcional. Indica a área do kernel DNU afetada.
- **`<descrição>`**: obrigatório. Resumo conciso no imperativo presente, sem
  letra maiúscula inicial e sem ponto final.
- **`<corpo>`**: opcional. Motivação e contraste com o comportamento anterior.
- **`<rodapé>`**: opcional. Referências a issues e indicação de *breaking changes*.

Exemplo:

```
feat(kernel): adicionar suporte a syscall de framebuffer

Implementa SYS_FB_INFO e SYS_FB_MAP para permitir que o ws_server
mapeie o framebuffer via userspace.

BREAKING CHANGE: o número da SYS_FB_MAP mudou de 49 para 50.
```

---

## Tipos permitidos

| Tipo | Quando usar | Exemplo |
|------|-------------|---------|
| `feat` | Nova funcionalidade para kernel ou userspace | `feat(gui): adicionar janela de bloco de notas` |
| `fix` | Correção de bug | `fix(kernel): corrigir page fault em mount_root_filesystems` |
| `docs` | Alterações apenas em documentação | `docs: atualizar commiting.md com escopos do kernel DNU` |
| `build` | Build system, scripts, dependências, CI | `build: atualizar Makefile do DNU para incluir dnu_pack` |
| `refactor` | Reestruturação sem mudar comportamento externo | `refactor: mover ABI para dnu/abi/` |
| `perf` | Melhoria de desempenho (tipo especial de refactor) | `perf(mm): reduzir iterações no alocador de frames` |
| `style` | Formatação, espaçamento, ponto e vírgula (sem mudar comportamento) | `style: padronizar indentação em main.rs` |
| `test` | Adição ou correção de testes | `test: adicionar teste de parse DEF no loader` |
| `chore` | Tarefas rotineiras (.gitignore, inicialização, manutenção) | `chore: atualizar .gitignore para ignorar target/` |
| `security` | Correções ou melhorias de segurança | `security: adicionar PBKDF2 ao banco de credenciais` |
| `ops` | Infraestrutura, deploy, CI/CD, monitoramento | `ops: adicionar workflow de build no GitHub Actions` |

---

## Escopos sugeridos — kernel DNU

O escopo ajuda a localizar rapidamente a área afetada. Use um dos seguintes
quando aplicável, refletindo os diretórios de `dnu/src/`:

| Escopo | Área |
|--------|------|
| `kernel` | Código geral do kernel e bring-up (`dnu/src/main.rs`, `boot.rs`) |
| `abi` | Contrato de ABI (`dnu/abi/`, `dnu/src/abi/`) |
| `arch` | Código específico de arquitetura (`dnu/src/arch/`: GDT, IDT, FPU, timer) |
| `boot` | Boot Limine, dzImage, entrada do kernel (`dnu/src/boot/`) |
| `console` | Console, framebuffer, serial, fonte PSF (`dnu/src/console/`) |
| `drivers` | Drivers de bloco (ATA), entrada (teclado/mouse), ACPI (`dnu/src/drivers/`) |
| `fs` | Sistemas de arquivos (RAMFS, DFS, VFS, GPT) (`dnu/src/fs/`) |
| `init` | Inicialização de userspace (`dnu/src/init/`) |
| `ipc` | Comunicação entre processos (`dnu/src/ipc/`) |
| `log` | Log do kernel (`dnu/src/log/`) |
| `mm` | Gerenciamento de memória e paginação (`dnu/src/mm/`) |
| `process` | Carga de DEF, escalonador, syscalls de processo (`dnu/src/process/`) |
| `subsystems` | Relatório e estado de subsistemas de boot (`dnu/src/subsystems/`) |
| `sync` | Primitivos de sincronização (`dnu/src/sync/`) |
| `time` | TSC, timer PIT, interrupções (`dnu/src/time/`) |
| `build` | Makefile, scripts de build, ferramentas de host (`Makefile`, `tools/`) |
| `docs` | Documentação (`docs/`, `README.md`) |

Exemplos com escopo:

```
fix(kernel): corrigir GPF ao acessar dzimage com HHDM offset
feat(mm): adicionar suporte a huge pages no alocador de frames
fix(drivers): corrigir leitura de setores no driver ATA
feat(fs): montar raiz persistente DFS a partir da partição GPT
build(tools): adicionar ferramenta dnu_pack ao Makefile
docs(abi): documentar novos números de SYS_*
```

---

## Breaking Changes

Um commit que introduz alterações que quebram compatibilidade **deve** usar o
indicador `!` antes dos dois pontos no tipo:

```
feat(abi)!: remover SYS_INT_0x80 em favor de syscall/sysret
```

O rodapé deve descrever o impacto:

```
BREAKING CHANGE: todos os programas userspace devem ser recompilados
e usar a instrução `syscall` em vez de `int 0x80`.
```

---

## Descrição

- Use o **imperativo, presente**: "adicionar" não "adicionado" nem "adiciona".
- Não capitalize a primeira letra.
- Não termine com ponto (`.`).
- Seja conciso (máximo recomendado: 72 caracteres).

Exemplos corretos:

```
feat: adicionar suporte a syscall de framebuffer
fix(kernel): corrigir wraparound em embedded_initramfs
```

Exemplos incorretos:

```
feat: Adicionado suporte a syscall de framebuffer
fix(kernel): Corrigido o bug do page fault.
docs: Atualiza a documentação da ABI.
```

---

## Corpo e Rodapé

O corpo é opcional e deve explicar o **porquê** da mudança, não apenas o **o quê**.

O rodapé é opcional, exceto para *breaking changes*. Pode conter referências a
issues (ex: `Closes #123`, `Fixes #456`) e deve iniciar com `BREAKING CHANGE:`
quando houver alteração que quebra compatibilidade.

---

## Exemplos práticos para o kernel DNU

```bash
# Nova funcionalidade no kernel
git commit -m "feat(kernel): adicionar handler de page fault ao IDT"

# Correção no gerenciamento de memória
git commit -m "fix(mm): corrigir vazamento de frames no allocator"

# Correção no driver de bloco
git commit -m "fix(drivers): corrigir leitura de setores no ATA"

# Documentação
git commit -m "docs: atualizar commiting.md com escopos do kernel DNU"

# Breaking change na ABI
git commit -m "feat(abi)!: mudar SYS_FB_MAP de 49 para 50

BREAKING CHANGE: ws_server e init devem ser recompilados contra a nova ABI."
```

---

## Regras de ouro do kernel DNU

Lembre-se das regras definidas para o desenvolvimento do DNU:

1. **Não modifique arquivos binários de terceiros** — fontes PSF e outros
   binários embarcados em `dnu/src/assets/` só devem ser alterados com
   avaliação de impacto no tamanho do kernel.
2. **GUI fica no userspace** — nunca coloque código de janela, compositor ou
   framebuffer no kernel DNU. O kernel expõe apenas syscalls de framebuffer;
   a interface gráfica roda fora do kernel.
3. **Sempre teste no QEMU antes de commitar** — se o commit afeta o boot, rode
   `make dzImage` e teste a inicialização em QEMU.
4. **Mantenha o `no_std` no kernel** — não use `std` em `dnu/src/`.
5. **A ABI é contrato do kernel** — alterações em `dnu/abi/` afetam todo o
   userspace; documente o impacto no rodapé do commit.
6. **O kernel compila em nightly** — o crate `limine` usa `#![feature]`;
   mantenha a toolchain atualizada via `rust-toolchain.toml`.

---

## Referências

- [Conventional Commits](https://www.conventionalcommits.org/)
- [Angular Commit Guidelines](https://github.com/angular/angular/blob/master/CONTRIBUTING.md)
