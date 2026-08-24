# Fase 0 — Reconhecimento

Data: 2026-08-23
Máquina: Windows 11 Pro 10.0.26100

---

## 1. Instalação do jogo

```
D:\SteamLibrary\steamapps\common\Resident Evil 0\
├── re0hd.exe            10.468.384 bytes   SHA256 67418C3A...4BC14B
├── dinput8.dll          (Ultimate ASI Loader — já instalado)
├── steam_api.dll
├── re0box.asi           (em scripts\)
├── re0box.ini
├── re0box.log
└── nativePC\
```

Cópia de trabalho: `work\re0hd.exe` (fora do diretório do jogo, conforme regra 1).

### re0box já está instalado

O mod de referência **já está ativo nesta instalação**, com `Leave=0` no `.ini`.
Isso importa para o projeto:

- Confirma que Ultimate ASI Loader funciona nesta máquina — a Fase 1 herda isso.
- O `re0box.log` já entregou a versão da build de graça (ver seção 3).
- **Conflito garantido** com este mod: os dois hookeiam o mesmo subsistema.
  Antes de qualquer teste in-game deste mod, remover `scripts\re0box.asi`.

O log registra uma exceção `E06D7363` (exceção C++ do MSVC) logo após "Patching
complete", com dump de registradores e módulos. Não investigado ainda — pode ser
first-chance benigna capturada pelo handler do próprio re0box. Não é bloqueante.

---

## 2. Cabeçalhos PE

Lidos com `tools\pe-recon.ps1` (PowerShell puro, sem dependências).

| Campo | Valor | Consequência |
|---|---|---|
| Machine | `IMAGE_FILE_MACHINE_I386` | build 32-bit obrigatória — confirmado |
| Magic | PE32 | — |
| ImageBase | `0x00400000` | bate com o presumido no CLAUDE.md |
| SizeOfImage | `0x00A66000` | módulo mapeado: `0x00400000`–`0x00E66000` |
| AddressOfEntryPoint | RVA `0x00A38310` (VA `0x00E38310`) | **cai na seção `.bind`**, não em `.text` |
| DllCharacteristics | `0x8100` | `NX_COMPAT` + `TERMINAL_SERVER_AWARE` |
| TimeDateStamp | `0x6799D40F` = 29/01/2025 07:09:03 UTC | consistente com a build Jan 28 2025 |

### ASLR: DESATIVADO

`IMAGE_DLLCHARACTERISTICS_DYNAMIC_BASE` (`0x0040`) **não** está setado.

Endereços absolutos são válidos direto em runtime, sem rebase — o módulo carrega
sempre em `0x00400000`. Confirmado empiricamente pelo dump de módulos do `re0box.log`:
`re0hd.exe 00400000-00E66000`.

Isso simplifica o código de patch, mas **não** justifica abandonar sigscan: endereço
literal continua quebrando na próxima atualização da Capcom.

### Seções

| Nome | VirtAddr | VirtSize | RawPtr | RawSize | Flags |
|---|---|---|---|---|---|
| `.text` | `0x00001000` | `0x008AF88B` | `0x00000400` | `0x008AFA00` | XR |
| `.rdata` | `0x008B1000` | `0x000B8A80` | `0x008AFE00` | `0x000B8C00` | R |
| `.data` | `0x0096A000` | `0x000CB778` | `0x00968A00` | `0x00061E00` | RW |
| `.rsrc` | `0x00A36000` | `0x000012F8` | `0x009CA800` | `0x00001400` | R |
| `.bind` | `0x00A38000` | `0x0002D7D0` | `0x009CBC00` | `0x0002D7D0` | XR |

Conversão VA → offset de arquivo:

```
.text    VA 0x00401000 - 0x00CB088B   offset = VA - 0x00400C00
.rdata   VA 0x00CB1000 - 0x00D69A80   offset = VA - 0x00401200
.data    VA 0x00D6A000 - 0x00E35778   offset = VA - 0x00401600
.rsrc    VA 0x00E36000 - 0x00E372F8   offset = VA - 0x0046B800
.bind    VA 0x00E38000 - 0x00E657D0   offset = VA - 0x0046C400
```

---

## 3. Build identificada

```
MasterRelease Jan 28 2025 16:45:59
```

Duas confirmações independentes:

1. String ASCII em claro no offset de arquivo `0x0091D374` (dentro de `.rdata`).
2. `re0box.log` linha 2: `Found game version: MasterRelease Jan 28 2025 16:45:59`.

**Portanto a tabela de endereços da build `Aug 28 2018` no CLAUDE.md não se aplica
como valor literal.** Serve apenas como mapa de "que funções existem".

Bônus: o offset `0x00966459` contém `MasterReleaseWin32\re0hd.pdb` — caminho do PDB
original. O PDB em si não é distribuído.

---

## 4. BLOQUEIO: `.text` está cifrado no disco (Steam DRM)

Este é o achado que muda o plano.

O executável tem uma seção `.bind` executável, e o entry point aponta para dentro dela.
Essa é a assinatura do **stub de DRM da Steam**: o `.bind` roda primeiro, descriptografa
`.text` em memória, e só então transfere controle para o entry point real do jogo.

Verificação direta, lendo `work\re0hd.exe` no VA `0x0050DC70`
(`get_character_bag` da build Jan 2025, segundo o re0box):

```
0x0050DC70  57 73 2D 1F F9 01 5B 3A  30 D7 0C A7 07 A6 C4 33  |Ws-...[:0......3|
0x0050DC80  4E 6D AB 85 AE 9C 97 0B  45 8B 1A F1 9D C0 15 0A  |Nm......E.......|
0x0050DC90  03 96 2A 3C CC 9D 38 CE  98 56 18 BD CB EF 27 8E  |..*<..8..V....'.|
0x0050DCA0  D6 94 87 FE C2 74 35 08  56 61 AA 29 61 22 EC E3  |.....t5.Va.)a"..|
```

Sem prólogo de função reconhecível, sem padrão x86 plausível — é ruído de alta entropia.

Nota importante: **apenas `.text` está cifrado.** As strings de `.rdata` estão em claro
(foi assim que a build foi identificada), e `.data` também parece intacta.

### O que isso quebra

- Desassemblar `re0hd.exe` do disco: **impossível** para código.
- Carregar o exe do disco no Ghidra / radare2 / IDA: só entrega lixo em `.text`.
- Sigscan feito sobre o arquivo em disco: **impossível**.

### O que isso NÃO quebra

- Sigscan **em runtime**, dentro do processo já rodando. Nesse ponto `.text` já foi
  descriptografado pelo stub, e o mod lê bytes normais. É exatamente assim que o re0box
  funciona, e é o caminho definitivo para o mod em si.
- Patches em runtime. Mesma razão.

### Caminho a seguir

Para as Fases 2 e 3 (validar layout do `Bag`, levantar xrefs) é preciso desassemblar
código real. Para isso é necessário **dumpar o módulo `re0hd.exe` da memória do processo
em execução**, depois que o stub descriptografou `.text`, e analisar esse dump.

Isso não contorna nem desabilita o DRM: o jogo continua exigindo Steam autenticada para
rodar, e o dump serve exclusivamente para leitura/análise local. O dump **não vai para o
repositório** (já coberto pelo `.gitignore`).

---

## 5. Saves

Localização real (Steam Cloud):

```
C:\Program Files (x86)\Steam\userdata\114203587\339340\remote\data0.bin
```

- Tamanho: **2.337.008 bytes** — bate exatamente com `UNMODDED_SAVE_SIZE` do re0box.
  Ou seja: save **vanilla**, sem dados extras anexados por mod ainda.
- SHA256: `5A828ED310886A1FC6DBAB77471C8E9B0130431118980CC0470DBD797CFD0B1A`

### Backup feito

```
backups\saves\2026-08-23_pre-mod\data0.bin          (hash verificado, idêntico)
backups\saves\2026-08-23_pre-mod\remotecache.vdf
```

Atenção: a pasta é `remote\`, ou seja **Steam Cloud está ativa**. Restaurar um backup
por cima do arquivo local pode ser sobrescrito pela nuvem no próximo start da Steam.
Antes de restaurar um save, desativar Steam Cloud para o jogo nas propriedades, ou
restaurar com a Steam fechada e conferir.

---

## 6. Toolchain

Estado encontrado na máquina:

| Ferramenta | Antes | Depois |
|---|---|---|
| git | `C:\Program Files\Git\cmd\git.exe` | inalterado |
| Visual Studio | Community 2026, MSVC 14.50.35717 em `D:\VisualStudio2026` | inalterado |
| cross-compiler x86 | `bin\Hostx64\x86\` presente | inalterado |
| rustc / cargo / rustup | ausentes | **instalados** — rustc 1.98.0 |
| target `i686-pc-windows-msvc` | ausente | **instalado** |
| Python | ausente (só o stub do WindowsApps) | ainda ausente |
| capstone / pefile | ausentes | ainda ausentes |
| Ghidra / rizin | ausentes | ainda ausentes |

O linker MSVC de 32 bits já existia via VS 2026, então o target `i686` não exigiu
instalação adicional além do próprio rustup.

Ferramentas escritas para este projeto, sem dependências externas:

- `tools\pe-recon.ps1` — parser de cabeçalho PE em PowerShell puro.
- `tools\hexdump-va.ps1` — dump de bytes por VA, com conversão VA → offset.

---

## 7. Veredito da Fase 0

| Critério de saída | Status |
|---|---|
| Build identificada | ✅ `MasterRelease Jan 28 2025 16:45:59` |
| Arquitetura confirmada | ✅ x86 32-bit, ImageBase `0x400000`, sem ASLR |
| Saves localizados e copiados | ✅ backup com hash verificado |
| `get_character_bag` confirmado por disassembly | ❌ **bloqueado por DRM** |

A Fase 0 não fecha pelo critério original. O último item exige um dump de memória em
runtime, que por sua vez exige que a DLL do mod já carregue no processo.

**Correção ao plano do CLAUDE.md:** a ordem das fases inverte. A Fase 1 (toolchain +
DLL que carrega) tem que vir antes de completar a Fase 0, porque o dump de memória
depende de ter código rodando dentro do processo — ou de um dumper externo.

Ordem revisada:

1. Fase 1 — DLL ASI mínima que carrega e loga. (toolchain já pronto)
2. Fase 0b — dump de `.text` da memória, análise estática do dump, confirmar
   `get_character_bag`.
3. Fase 2 em diante, conforme o plano original.
