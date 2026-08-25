# RE0 Inventory Expansion Mod — Contexto do Projeto

## Objetivo

Criar um mod para **Resident Evil 0 HD Remaster (PC/Steam)** que aumente o número de
slots de inventário por personagem, atualmente fixo em 6.

Stack alvo: **Rust**, compilado como plugin ASI 32-bit — mesma stack do projeto de
referência. Ver seção "Stack".

---

## Regras inegociáveis

1. **NUNCA modificar a instalação da Steam diretamente.** Copie `re0hd.exe` para uma
   pasta de trabalho local e analise a cópia. Toda escrita em disco acontece fora do
   diretório do jogo, exceto no momento explícito de instalar o mod para teste.
2. **NUNCA commitar `re0hd.exe`, saves, arquivos `.arc`, `.gmd` ou qualquer asset do
   jogo.** Adicione ao `.gitignore` antes do primeiro commit. Material da Capcom não vai
   para repositório público.
3. **Backup dos saves antes de qualquer teste in-game.** Mods que tocam no formato de
   save podem corromper progresso. Localize a pasta de saves e copie antes de rodar.
4. **Não assumir endereços.** Todo endereço neste documento é referência de terceiro,
   válido para builds específicas. Sempre validar contra o executável real antes de
   escrever um patch.
5. **Não desabilitar nem contornar verificação de Steam/DRM.** O mod pressupõe cópia
   legítima. Isso não é escopo do projeto.

---

## O jogo

| Item | Valor |
|---|---|
| Executável | `re0hd.exe` |
| Arquitetura | x86 (32-bit) |
| Engine | MT Framework (Capcom) |
| Assets | `nativePC\arc\**\*.arc` |
| Localização típica | `steamapps\common\<pasta do RE0>\` |

Builds conhecidas, identificadas por string ASCII em seção read-only do módulo:

- `MasterRelease Aug 28 2018 14:42:14`
- `MasterRelease Jan 28 2025 16:45:59`

**Tarefa zero:** descobrir qual dessas (ou qual outra) está instalada na máquina. Se for
uma terceira build, toda a tabela de endereços abaixo é inútil como valor literal e serve
apenas como mapa de "que funções existem".

---

## Projeto de referência: `descawed/re0box`

<https://github.com/descawed/re0box> — mod de item box para RE0 HD, escrito em Rust,
distribuído como plugin ASI via Ultimate ASI Loader. É a melhor documentação existente do
subsistema de inventário do RE0.

**Status legal:** o repositório **não tem arquivo LICENSE**. Isso significa "all rights
reserved" por padrão. Consequências práticas:

- Usar como **referência técnica** (endereços, layout de struct, comportamento observado)
  é aceitável — são fatos sobre o jogo, não expressão criativa do autor.
- **Não copiar código.** Este projeto usa a mesma linguagem e a mesma stack que o
  re0box, então essa regra exige disciplina ativa: ler para entender, fechar o arquivo,
  escrever do zero. Nada de copy-paste com nomes trocados.
- Antes de publicar qualquer coisa derivada, abrir issue no repo e falar com o autor. Ele
  está ativo (release v0.5.2 em fev/2026).

### Arquitetura do re0box (o que ele faz e por que funciona)

O insight central: **ele não altera a struct de inventário do jogo.** Mantém o `Bag` de 6
slots intacto e usa o painel do parceiro como uma **janela deslizante** sobre um `Vec`
próprio, que vive na memória do mod.

```rust
// src/inventory.rs — layout que espelha a struct do jogo
pub const BAG_SIZE: usize = 6;

#[repr(C)]
pub struct Item {
    id: i32,
    count: i32,
}   // 8 bytes

#[repr(C)]
pub struct Bag {
    unknown00: i32,            // +0x00
    items: [Item; 6],          // +0x04 .. +0x33
    personal_item: Item,       // +0x34
    equipped_item_index: i32,  // +0x3C
}   // 64 bytes total
```

```rust
pub struct ItemBox {
    is_open: bool,
    items: Vec<Item>,   // a caixa real, tamanho arbitrário
    index: usize,       // offset da janela dentro de items
    view: Bag,          // os 6 que o jogo enxerga
}
```

`update_view()` copia 6 itens de `items[index..index+6]` para `view`.
`update_from_view()` copia de volta. O jogo lê e escreve em `view` achando que é um bag
normal. Nunca sabe que existe mais coisa.

### Itens de dois slots (crítico)

RE0 tem itens que ocupam 2 slots. O segundo slot recebe um item-filler de id `180`.

```rust
const SLOT_TWO: i32 = 180;
const TWO_SLOT_ITEMS: [i32; 9] = [
    5,   // hunting gun
    6,   // shotgun
    7,   // grenade launcher (grenade rounds)
    8,   // grenade launcher (flame rounds)
    9,   // grenade launcher (acid rounds)
    11,  // sub-machine gun
    12,  // arma inválida, sem nome/ícone/modelo
    23,  // rocket launcher
    104, // hookshot
];
```

Consequências que restringem o design:

- O inventário é tratado como **linhas de 2**. Índices de scroll são forçados a múltiplos
  de 2 (`new_index = (index + offset + 1) & !1`).
- Primeira metade de item duplo em índice ímpar = estado inválido.
- `SLOT_TWO` em índice par = estado inválido.
- Qualquer contagem de slots escolhida **tem que ser par**.
- Existe lógica dedicada de reparo (`organize`, `fix_misaligned`, `is_broken`,
  `is_organized`) porque o jogo às vezes deixa o bag em estado quebrado no meio de uma
  troca.

### Constantes de save

```
NUM_SAVE_SLOTS      = 20
UNMODDED_SAVE_SIZE  = 2337008   // 20 slots + header/metadata
MAGIC               = "IBOX"    // marcador dos dados extras do mod
MOVE_SELECTION_SOUND = 2050
FAIL_SOUND           = 2053
```

O re0box anexa os dados da caixa depois do save vanilla, marcado por magic. Uninstall com
save = perda dos itens da caixa. Mesmo problema vai existir para slots extras.

### Tabela de endereços — build `Aug 28 2018`

VAs absolutos, image base presumida `0x400000`. **Validar antes de usar.**

```
get_character_bag            0x0050DA80   <-- ponto de partida do seu trabalho
get_partner_bag              0x004DC8B0
get_partner_bag_org          0x004DC625
draw_bags                    0x005E6ED0
organize_end1                0x004DADC7
organize_end2                0x004DADDA
scroll_up_check              0x005E386A
scroll_down_check            0x005E3935
scroll_left_check            0x005E39F1
scroll_right_check           0x005E3AFD
scroll_right_two_check       0x005E3B5A
exchange_size_check          0x005E3E94
shaft_check                  0x005E3D73
prepare_inventory            0x005D71D0
inventory_menu_start         0x005E1B86
inventory_menu_close         0x005D8983
inventory_change_character   0x005E2BCA
inventory_open_animation     0x005E1B4F
play_menu_animation          0x005DBDF0
leave_sound_arg              0x005E3634
leave_menu_state             0x005E363D
get_partner_character        0x0066DEC0
play_sound                   0x005EE920
set_room_phase               0x00610C20
no_ink_ribbon                0x0057AD54
has_ink_ribbon               0x0057AD19
typewriter_choice_check      0x0057ADA7
typewriter_phase_set         0x0057ADE6
new_game                     0x0041249C
load_slot                    0x006125F1
post_load                    0x008B5975
save_slot                    0x006134E9
steam_save                   0x008B5CC1
steam_remote_storage         0x00CB1440
msg_load1                    0x0040864E
msg_load2                    0x005D6471
msg_load3                    0x005D67E1
sub_522a20                   0x00522A20
sub_4db330                   0x004DB330
sub_6fc610                   0x006FC610
ptr_dcdf3c                   0x00DCDF3C
ptr_dd0bd0                   0x00DD0BD0
```

Build `Jan 28 2025` tem os mesmos símbolos em endereços deslocados (ex.:
`get_character_bag = 0x0050DC70`). A tabela completa está em `src/game.rs` do re0box.

Nem todos são entry points de função — vários (`*_check`, `organize_end*`,
`leave_sound_arg`) são **patch sites no meio de funções**, onde ele sobrescreve bytes e
desvia para um trampolim com asm escrito à mão.

---

## Stack: Rust, plugin ASI

Mesma stack do re0box. Escolha deliberada: é o único caminho para esse jogo com
precedente comprovado, e mantém o projeto de referência 1:1 comparável.

### O que o mod exige

- DLL carregada dentro de um processo nativo x86
- Detours em entry points de função
- **Patches mid-function** com convenções de registrador não-padrão (o código é C++
  compilado, `__thiscall` com `this` em `ecx`, valores vivos em `edi`/`esi`)
- Escrita em páginas de código (`VirtualProtect` → `PAGE_EXECUTE_READWRITE`)
- Hook em save/load para estender o formato de save

Rust com `no_std`-ish discipline nos hooks atende tudo isso sem runtime intermediário: a
DLL é código nativo, sem GC, sem marshaling, sem pausa imprevisível num hook chamado por
frame.

### Configuração de build

- **Target obrigatório: 32-bit.** `i686-pc-windows-msvc` (preferido) ou
  `i686-pc-windows-gnu`. Build x64 não carrega no processo — o jogo é x86.
- `crate-type = ["cdylib"]` no `Cargo.toml`
- Build: `cargo build --release --target=i686-pc-windows-msvc`
- Distribuição: renomear o `.dll` gerado para `.asi` e colocar em `scripts\` na pasta do
  jogo, com o **Ultimate ASI Loader** (`dinput8.dll`) na raiz

### Crates de referência

O re0box usa esse conjunto, e vale seguir:

- `windows` — `VirtualProtect`, `VirtualQuery`, APIs de módulo/processo
- `memchr` — busca de padrão de bytes (`memmem`) para sigscan e detecção de versão
- `binrw` — serialização binária declarativa, usada no formato de save
- `anyhow` — tratamento de erro
- `log` + um backend de arquivo — logging com nível configurável por `.ini`

### Trade-offs honestos

- Você não é dev Rust no dia a dia. A curva aqui não é a linguagem em si, é `unsafe` +
  ponteiros crus + FFI. Espere atrito nas primeiras semanas.
- Em compensação: o re0box existe como referência de arquitetura escrita exatamente nessa
  stack, resolvendo exatamente os mesmos problemas de baixo nível.
- Escrever trampolim de x86 à mão é inevitável em qualquer linguagem aqui. Rust não
  facilita nem atrapalha — `patch.rs` do re0box mostra o padrão (montar bytes de
  `call`/`jmp`/`jl`/`jge` calculando offsets relativos manualmente).

### Cuidado redobrado com a licença

Mesma linguagem, mesma stack, mesmos problemas — a tentação de copiar do re0box é alta, e
o repositório **não tem LICENSE**. Reimplemente a partir do entendimento, não do
copy-paste. Endereços e layout de struct são fatos sobre o jogo; a implementação em Rust
do autor é obra dele.

---

## Instalação de ferramentas

**Esta é a máquina pessoal do usuário, não um container descartável.** Nada é instalado
sem pedir.

Regras:

1. **Antes de instalar qualquer coisa, verifique se já existe.** `rustc --version`,
   `cargo --version`, `python --version`, `rustup target list --installed`. Metade dessa
   lista provavelmente já está aí — o usuário é dev.
2. **Peça autorização explícita antes de cada instalação**, dizendo o que é, por que
   precisa, tamanho aproximado e como desinstalar depois. Uma pergunta por ferramenta, não
   uma lista genérica de "posso instalar as dependências?".
3. **Nada de instalação global silenciosa.** Preferir escopo de projeto sempre que der
   (venv para Python, `rustup` já é por usuário).
4. **Não altere PATH, variáveis de ambiente do sistema, nem configuração de IDE** sem
   avisar.
5. Se uma ferramenta pesada (Ghidra, ~1GB + dependência de Java) for necessária,
   **apresente a alternativa leve primeiro** e deixe o usuário escolher.

### O que provavelmente precisa ser instalado

| Ferramenta | Para quê | Peso | Provável já ter? |
|---|---|---|---|
| `rustup` + toolchain stable | compilar o mod | ~500MB com toolchain MSVC | Talvez não |
| target `i686-pc-windows-msvc` | build 32-bit — **obrigatório** | ~100MB | Não |
| Visual Studio Build Tools (C++) | linker MSVC exigido pelo target | grande, mas comum em máquina Windows de dev | Possivelmente |
| Python + `capstone`, `pefile` | análise estática do PE | leve | Python sim, os pacotes não |
| Ultimate ASI Loader | carregar o `.asi` no jogo | ~1MB, um único DLL | Não |
| Ghidra ou radare2/rizin | xrefs (Fase 3) | Ghidra ~1GB; rizin ~50MB | Não |

Comandos de referência (**não rodar sem autorização**):

```
rustup target add i686-pc-windows-msvc
pip install capstone pefile
```

Se o usuário recusar alguma instalação, **não trave o projeto** — proponha o caminho
alternativo. Sem Ghidra, dá para ir longe com capstone + análise manual de xrefs; é mais
lento, mas funciona.

---

## Ferramentas de análise

- **`pefile`** — ler cabeçalhos PE, seções, `ImageBase`, `DllCharacteristics`, conversão
  VA ↔ offset de arquivo.
- **`capstone`** — desassemblar trechos específicos. Ideal para "me mostra 40 instruções
  a partir de 0x0050DA80".
- **Ghidra headless** (`analyzeHeadless`) — necessário para **xrefs**. Capstone sozinho
  não resolve "quem chama essa função". Se disponível, criar um projeto Ghidra do exe e
  dirigi-lo por script.
- **radare2/rizin** — alternativa mais leve ao Ghidra, com xrefs razoáveis e scriptável
  via r2pipe. Aceitável se Ghidra não estiver instalado.

### Conversão de endereço

Endereços neste doc são **VAs**, presumindo `ImageBase = 0x400000`.

```
file_offset = VA - ImageBase - section.VirtualAddress + section.PointerToRawData
```

**Verificar `ImageBase` real com pefile — não presumir.**

### ASLR

O re0box aplica patches em endereços absolutos sem rebase. Isso só funciona se o
executável **não** tiver ASLR ativo. Checar a flag `IMAGE_DLLCHARACTERISTICS_DYNAMIC_BASE`
em `OPTIONAL_HEADER.DllCharacteristics`. Se estiver ativa, todo endereço precisa ser
somado ao base do módulo em runtime. Isso muda o design do código de patch — descobrir
cedo.

---

## Plano de fases

Não pular fases. Cada uma valida a anterior.

### Fase 0 — Reconhecimento

1. Localizar a instalação do RE0 e copiar `re0hd.exe` para a pasta de trabalho.
2. `pefile`: confirmar `Machine = IMAGE_FILE_MACHINE_I386`, ler `ImageBase`, listar
   seções, checar flag de ASLR.
3. Procurar a string `MasterRelease ` no binário e extrair a data. Determinar a build.
4. Se a build for uma das duas conhecidas: desassemblar `get_character_bag` no endereço
   correspondente e confirmar que parece um getter (prólogo curto, retorno de ponteiro).
   Se não bater, a tabela não se aplica — partir para sigscan.
5. Registrar tudo em `docs/00-recon.md`.

**Só avançar quando a build estiver identificada e `get_character_bag` confirmado.**

### Fase 1 — Toolchain

1. Instalar toolchain Rust com target 32-bit (ver seção "Instalação de ferramentas").
2. Criar o crate com `crate-type = ["cdylib"]`, compilar para `i686-pc-windows-msvc`.
3. `DllMain` que só escreve num arquivo de log: "carregado" + base address do módulo.
4. Renomear para `.asi`, instalar com Ultimate ASI Loader, subir o jogo, confirmar o log.

Objetivo: provar que a DLL carrega e escreve antes de investir em qualquer lógica. Se
esta fase falhar, nada depois importa.

### Fase 2 — Validar o layout do Bag

1. Hook em `get_character_bag`, logar o ponteiro retornado.
2. Dumpar 64 bytes a partir dele.
3. Correlacionar com o inventário visível in-game: pegar um item conhecido, ver o id
   aparecer no dump, largar, ver sumir.
4. Confirmar offsets de `personal_item` (+0x34) e `equipped_item_index` (+0x3C).

Não prosseguir com layout presumido. Confirmar empiricamente.

### Fase 3 — Levantamento de xrefs

Esta fase determina a viabilidade de tudo.

1. Todos os xrefs de `get_character_bag` e `get_partner_bag`.
2. Para cada um, classificar:
   - **Itera o bag?** (loop com bound 6 / comparação `cmp ... 6` / `cmp ... 5`)
   - **Aritmética de offset hardcoded?** (`+0x34`, `+0x3C`, stride `0x30` = 6×8)
   - **Só lê um slot específico?** (provavelmente inofensivo)
3. Produzir `docs/03-xrefs.md` com a lista completa e a contagem de sites que assumem 6.

**Esse número decide a Fase 4.**

### Fase 4 — Decisão de arquitetura

Duas opções. Escolher com base na Fase 3, documentar a decisão e o motivo.

**Opção A — Janela deslizante** (mesmo truque do re0box, aplicado ao bag do jogador)

O bag do jogo continua com 6 slots e vira uma view sobre um backing store de N slots no
mod. Hookear os `scroll_*_check` para o painel do jogador rolar como o da caixa.

- Prós: layout de memória intocado, nada downstream quebra, herda toda a engenharia
  reversa já feita.
- Contra: código do jogo que **varre** o bag só enxerga os 6 visíveis. "Tenho a chave?",
  auto-reload procurando munição, check de inventário cheio. Cada um vira um hook
  adicional que precisa consultar o store completo. A Fase 3 diz quantos são.

**Opção B — Realocar e expandir a struct**

Alocar um bag maior no mod, fazer `get_character_bag` retornar esse ponteiro.

- Prós: o jogo enxerga tudo, buscas funcionam nativamente.
- Contra: `personal_item` e `equipped_item_index` estão em offsets fixos logo após o
  array. Com array maior eles saem do lugar, e **todo** acesso a esses offsets no
  disassembly precisa ser corrigido. Além de qualquer struct pai que embuta o `Bag`
  inline e assuma tamanho 64.

Viés inicial: **Opção A**, a menos que a Fase 3 revele um número inviável de sites de
varredura. Mas decidir com dado, não com viés.

### Fase 5 — Implementação

Incremental, testando in-game a cada passo:

1. 8 slots antes de 12. Menor delta, mesmos problemas estruturais.
2. Manter paridade — nunca número ímpar.
3. Testar itens de 2 slots exaustivamente a cada mudança. É onde vai quebrar.
4. Testar troca de itens entre Rebecca e Billy.
5. Só então mexer no formato de save.

### Fase 6 — Save e UI

- Estender o save preservando compatibilidade de leitura com saves vanilla.
- Magic marker próprio, não reusar `IBOX`.
- Definir comportamento quando o mod é desinstalado com itens nos slots extras.
- Scroll visual no painel do jogador.

---

## Diretrizes de trabalho

- **Não escrever patch antes de validar o endereço.** Sempre: desassemblar → confirmar que
  o código faz o que se espera → só então patchear.
- **Preferir sigscan a endereço literal.** Endereço literal quebra na próxima atualização
  da Capcom. Padrão de bytes sobrevive à maioria.
- **Logging generoso desde o dia um.** Sem debugger decente, o log é a única visibilidade.
  Níveis configuráveis por arquivo de config, como o re0box faz.
- **Um hook por commit.** Quando quebrar — e vai — o bisect precisa ser trivial.
- **Documentar cada descoberta em `docs/`.** Endereço encontrado, o que a função faz,
  como foi confirmado. Isso vira o mapa que não existe em lugar nenhum.
- **Quando algo não bater com este documento, este documento está errado.** Ele foi
  montado a partir de código de terceiro e leitura estática. O executável na máquina é a
  fonte da verdade. Corrigir aqui e seguir.

## Custo de agentes

Medido nesta sessão: fan-out de subagentes gastou 2,2M tokens, contra 5k do contexto
fixo. É onde o orçamento vai, então é a única coisa que vale governar.

- **Fan-out não é padrão.** Abrir vários agentes só quando o usuário pedir, ou quando um
  erro já custou uma rodada de teste in-game dele. Fora isso, investigar direto.
- **Trabalho mecânico vai em modelo barato.** Localizar chamador, conferir assinatura,
  ler arquivo conhecido: `model: haiku`. Só análise que decide arquitetura merece o
  modelo caro.
- **Um lookup não é um agente.** Endereço, símbolo ou função que já se sabe onde está:
  `grep`, ou o `analyzer` deste repo.
- **Fan-out que valeu:** auditoria dos 30 patches do re0box (achou o bug do helper de
  troca `0x004DDFC0`, que eu tinha errado sozinho duas vezes) e o code review da
  persistência (achou perda de dados silenciosa). **Que não valeu:** 9 agentes para mapear
  save/load — 8 morreram no limite e o resultado veio pela metade.

## Perguntas em aberto

- Quantos slots é o alvo final? 8, 12, configurável?
- O mod deve conviver com o re0box instalado, ou são mutuamente exclusivos? (Ambos
  hookeiam o mesmo subsistema — provável conflito.)
- Slots extras valem para ambos os personagens, ou é configurável por personagem?
