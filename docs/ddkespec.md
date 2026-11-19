# Especificação de Extensões de Kernel Dinâmicas do DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** Kernel DNU (Dreamcore) e sistema de arquivos do ImagineOS

---

## 1. Introdução

O **DNU Dynamic Kernel Extension (DDKE)** é o mecanismo pelo qual o ImagineOS lida com drivers de dispositivos de forma dinâmica, sem exigir recompilação do kernel.

Em vez de tratar drivers como "módulos" fortemente acoplados ao núcleo, o DNU adota o modelo de **extensões**: pacotes `.ddke` que adicionam ou sobrescrevem funcionalidades de **drivers genéricos** já presentes no kernel. Isso permite:

- Instalar drivers após o boot (por meio de um instalador gráfico, por exemplo)
- Remover ou desativar drivers sem reiniciar o sistema
- Manter o kernel funcional mesmo quando uma extensão falha, graças ao fallback no driver genérico

---

## 2. Modelo: Driver Genérico + Extensão DDKE

### 2.1 Driver Genérico

Cada categoria de hardware (bloco, vídeo, entrada, rede, etc.) possui um **driver genérico** compilado diretamente no kernel DNU. Ele:

- Implementa o mínimo necessário para o dispositivo funcionar (leitura/escrita básica, modo de vídeo padrão, etc.)
- Nunca trava o kernel: em caso de falha, retorna erro e continua operando
- Expõe uma **tabela de funções genéricas** que extensões podem sobrescrever

### 2.2 Extensão DDKE

Uma extensão é um pacote `.ddke` que:

- É compilado no formato **DEF** (Dreamcore Executable Format), igual aos programas de userspace
- Contém propriedades específicas que identificam o arquivo como driver e não como programa comum
- Implementa a interface exposta por um driver genérico alvo
- Pode desativar funções do driver genérico e injetar funções próprias
- É carregado dinamicamente pelo kernel em tempo de execução

---

## 3. Formato de Arquivo `.ddke`

### 3.1 Base em DEF

Os arquivos `.ddke` são empacotados no formato **DEF** (Dreamcore Executable Format), reutilizando a infraestrutura de carregamento já existente no DNU.

### 3.2 Propriedades de Driver (metadados)

O cabeçalho do DEF pode conter campos de propriedades que diferenciam um `.ddke` de um programa comum. Caso o cabeçalho DEF atual não possua esse campo, ele deverá ser adicionado em uma implementação futura.

Propriedades sugeridas para identificar um DDKE:

| Campo | Descrição | Exemplo |
|-------|-----------|---------|
| `driver_type` | Tipo de dispositivo alvo | `block`, `video`, `input`, `ata`, `acpi` |
| `driver_id` | Identificador único da extensão | `ata_nvme_01`, `video_virtio_01` |
| `generic_target` | Driver genérico que esta extensão estende | `block.ata`, `video.fb` |
| `ddke_version` | Versão da extensão | `1.0.0` |
| `ddke_abi_version` | Versão da ABI de extensão esperada pelo kernel | `1` |

### 3.3 Assinatura e Segurança (implementação futura)

Para impedir o carregamento de extensões maliciosas ou corrompidas, o cabeçalho DEF do `.ddke` pode conter um campo de **assinatura digital**.

| Campo | Descrição | Status |
|-------|-----------|--------|
| `ddke_signature` | Assinatura criptográfica do pacote | Futuro (se não existir no DEF) |
| `ddke_signature_algoritmo` | Algoritmo usado (ex: RSA, Ed25519) | Futuro |
| `ddke_certificado` | Certificado ou chave pública do autor | Futuro |

O kernel deve verificar a assinatura antes de carregar a extensão. Se a verificação falhar, o DDKE é rejeitado e o driver genérico permanece ativo.

> **Nota:** Se o formato DEF atual não possuir esses campos, eles devem ser considerados como planejamento para versões futuras do formato e do carregador DDKE.

---

## 4. Fluxo de Carregamento

```
1. Um arquivo .ddke é colocado em /Drivers/ ou /Boot/Kernel/drivers/
2. O kernel (ou um serviço de userspace) detecta o novo arquivo
3. O carregador DDKE:
   a. Lê o arquivo como DEF
   b. Verifica as propriedades de driver no cabeçalho
   c. (Futuro) Verifica a assinatura digital
   d. Aloca memória no kernel para o código da extensão
   e. Resolve símbolos contra a tabela de funções genéricas
   f. Injeta as funções da extensão no driver genérico alvo
   g. Registra a extensão no registro do sistema (/Config/Registry/)
   h. Ativa o driver estendido
4. Se qualquer etapa falhar:
   - A extensão é desativada
   - O driver genérico continua funcionando
   - O erro é registrado para diagnóstico
```

---

## 5. Registro e Isolamento

### 5.1 Registro de Extensões

Extensões carregadas são registradas em:

```
/Config/Registry/
```

Cada registro contém o ID da extensão, o driver genérico alvo, o status (ativo/inativo) e o caminho do arquivo `.ddke`.

### 5.2 Isolamento de Falha

Uma extensão DDKE nunca deve causar um kernel panic direto. Se uma função injetada falhar:

1. O kernel captura a exceção ou erro
2. A extensão é marcada como inativa
3. O controle retorna ao driver genérico
4. O sistema continua operando, possivelmente com funcionalidade reduzida

---

## 6. Locais no Sistema de Arquivos (conforme IFHSpec)

| Caminho | Uso |
|---------|-----|
| `/Boot/Kernel/` | DDKE críticos empacotados com o kernel |
| `/Drivers/` | DDKE de sistema instalados após o boot |
| `/System/Users/<usuario>/local#Libraries/` | DDKE de bibliotecas de usuário |
| `/Config/Registry/` | Registro de extensões carregadas |
| `/Libraries/System/` | Bibliotecas de sistema relacionadas a drivers |

---

## 7. Syscalls Sugeridas (futuro)

| Syscall | Descrição |
|---------|-----------|
| `SYS_DDKE_LOAD` | Carrega uma extensão DDKE a partir de um caminho |
| `SYS_DDKE_UNLOAD` | Descarrega e desativa uma extensão por ID |
| `SYS_DDKE_LIST` | Lista todas as extensões registradas |
| `SYS_DDKE_QUERY` | Consulta o status de uma extensão específica |

---

## 8. Exemplos de Pacotes `.ddke`

| Arquivo | Driver Genérico Alvo | Propriedade `driver_type` |
|---------|----------------------|---------------------------|
| `/Drivers/ata_nvme.ddke` | `block.ata` | `block` |
| `/Drivers/video_virtio.ddke` | `video.fb` | `video` |
| `/Drivers/input_ps2_ext.ddke` | `input.ps2` | `input` |
| `/Boot/Kernel/drivers/ata_generic.ddke` | `block.ata` (crítico) | `block` |

---

## 9. Implementação Futura vs Atual

| Recurso | Status Atual | Planejado |
|---------|--------------|-----------|
| Formato DEF para programs | ✅ Implementado | — |
| Arquivos `.ddke` baseados em DEF | ❌ Não implementado | Futuro |
| Campo de propriedades de driver no cabeçalho DEF | ❌ Não confirmado | Futuro (se não existir) |
| Campo de assinatura digital no cabeçalho DEF | ❌ Não implementado | Futuro |
| Carregador DDKE no kernel DNU | ❌ Não implementado | Futuro |
| Driver genérico com tabela de funções sobrescritas | ❌ Não implementado | Futuro |

---

## 10. Histórico de Versões

| Versão | Data | Alterações |
|--------|------|------------|
| 1.0 | 2026-11-10 | Primeira especificação de DDKE para o DNU |