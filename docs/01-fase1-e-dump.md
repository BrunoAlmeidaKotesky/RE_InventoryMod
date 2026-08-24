# Fase 1 — Toolchain, DLL carregando, e o dump de `.text`

Data: 2026-08-23

---

## 1. Toolchain final

Diferente do que o CLAUDE.md previa, o alvo **não** é `i686-pc-windows-msvc`.

| Item | Decisão | Motivo |
|---|---|---|
| Target | `i686-pc-windows-gnu` | O Windows SDK não está instalado; só existe o compilador C++ do VS 2026, sem `kernel32.lib`. O target gnu traz linker e import libs próprios. |
| Toolchain | `stable-i686-pc-windows-gnu` (fixada) | A toolchain host MSVC não tem `dlltool`, exigido pelo target gnu. |
| Dependências do crate | **nenhuma** | Ver abaixo. |

### Por que o crate não tem dependências

A primeira tentativa usou `windows-sys`. Ele resolve as APIs do Win32 por
`raw-dylib`, e no target gnu isso passa pelo `dlltool` que vem com o Rust — que
falha, porque não tem um assembler para chamar:

```
error: dlltool could not create import library ...
       dlltool.exe: CreateProcess
```

O mod usa nove funções do Win32 no total. Declarar as nove à mão em
`src/win32.rs` com `#[link(name = "kernel32")]` usa o caminho clássico de import
library, que funciona, e de quebra deixa o crate sem nenhuma dependência
externa.

### Armadilha de build

Compilar pelo Git Bash falha com uma mensagem enganosa:

```
link: extra operand '...rcgu.o'
```

Isso é o `link` do GNU coreutils sendo escolhido no lugar do linker da
toolchain. **Compilar sempre pelo PowerShell**, via `tools\build.ps1`.

---

## 2. Resultado do primeiro carregamento in-game

O mod carregou e escreveu o log completo:

```
[INFO] Game module: base 0x00400000, size 0x00A66000, 5 sections.
[INFO] Section .bind present: the executable is packed by Steam DRM.
[DEBUG] Code section decrypted after 0 ms.
[INFO] Game build: MasterRelease Jan 28 2025 16:45:59
[INFO] Dumped '.text' to ... (9107595 bytes, base 0x00401000).
```

Confirmações:

- Os endereços de runtime batem exatamente com os do arquivo em disco.
  ASLR desativado, como previsto na Fase 0.
- `Code section decrypted after 0 ms`: o Ultimate ASI Loader só carrega os
  `.asi` **depois** que o stub de DRM roda. A espera implementada em
  `debug/dump.rs` acabou não sendo necessária nesta configuração, mas continua
  no código porque a ordem de carga não é garantida.
- O dump tem 9.107.595 bytes = `0x8AF88B`, exatamente o `VirtualSize` de
  `.text`.

---

## 3. Achado principal: `get_partner_bag` em `0x004DC8B0`

Desassemblado do dump, decodificado à mão:

```asm
0x004DC8B0  8B 44 24 04     mov  eax, [esp+4]        ; arg: id do personagem
0x004DC8B4  83 F8 01        cmp  eax, 1
0x004DC8B7  74 1F           je   0x004DC8D8
0x004DC8B9  83 F8 02        cmp  eax, 2
0x004DC8BC  74 1A           je   0x004DC8D8
0x004DC8BE  83 F8 03        cmp  eax, 3
0x004DC8C1  74 15           je   0x004DC8D8
0x004DC8C3  83 F8 05        cmp  eax, 5
0x004DC8C6  74 0A           je   0x004DC8D2
0x004DC8C8  83 F8 07        cmp  eax, 7
0x004DC8CB  74 05           je   0x004DC8D2
0x004DC8CD  33 C0           xor  eax, eax            ; id desconhecido -> NULL
0x004DC8CF  C2 04 00        ret  4
0x004DC8D2  8D 41 60        lea  eax, [ecx+0x60]     ; ids 5, 7
0x004DC8D5  C2 04 00        ret  4
0x004DC8D8  8D 41 20        lea  eax, [ecx+0x20]     ; ids 1, 2, 3
0x004DC8DB  C2 04 00        ret  4
```

Leitura:

- Convenção `__thiscall`: `this` em `ecx`, um argumento na pilha, `ret 4`.
- Retorna um ponteiro para dentro do próprio objeto: `this+0x20` ou `this+0x60`.
- Ids 1, 2 e 3 caem no mesmo bag; 5 e 7 no outro. Provavelmente variantes do
  mesmo personagem (trajes alternativos, modo Wesker).

### Por que isso decide a Fase 4

`0x60 - 0x20 = 0x40` = **64 bytes**, exatamente o tamanho presumido do `Bag`.

Ou seja: **os dois bags são campos inline e adjacentes dentro do mesmo objeto
pai.** Não são ponteiros para alocações separadas.

Consequência direta: a **Opção B** do plano (realocar e expandir a struct no
lugar) está descartada. Um array maior no bag em `+0x20` invadiria o bag em
`+0x60`, e o offset `+0x60` está gravado como literal no código. Toda a
aritmética de offset do objeto pai teria que ser reescrita.

Resta a **Opção A — janela deslizante**, que era o viés inicial do plano.
Confirmada por dado, não por preferência.

---

## 4. `0x0050DC70` — wrapper com assert

```asm
0x0050DC70  83 EC 08              sub  esp, 8
0x0050DC73  56                    push esi
0x0050DC74  8B 35 44 BF DC 00     mov  esi, [0x00DCBF44]   ; optional global
0x0050DC7A  57                    push edi
0x0050DC7B  8B F9                 mov  edi, ecx            ; salva this
0x0050DC7D  85 F6                 test esi, esi
0x0050DC7F  75 1B                 jne  0x0050DC9C
0x0050DC81  68 38 4F CB 00        push 0x00CB4F38
0x0050DC86  68 8B 01 00 00        push 0x18B               ; linha 395
0x0050DC8B  68 4C 4F CB 00        push 0x00CB4F4C
0x0050DC90  E8 BB 42 EF FF        call 0x00401F50          ; assert
0x0050DC95  8B 74 24 14           mov  esi, [esp+0x14]
0x0050DC99  83 C4 0C              add  esp, 0x0C
0x0050DC9C  8B CF                 mov  ecx, edi            ; restaura this
0x0050DC9E  E8 ...                call ...
```

As strings em `.rdata` (nunca cifradas) são:

```
0x00CB4F4C : D:\BH0_PC_KANTAIJI\Game\lib\tsl/optional.h
0x00CB4F38 : is_initialized()
```

Confirma que o dump é código real da Capcom, e revela o caminho de build
original do projeto (`BH0_PC_KANTAIJI`). A função desreferencia um
`tsl::optional` global em `0x00DCBF44`, com assert de inicialização.

Ponteiro global a investigar: **`0x00DCBF44`**.

---

## 5. Estado das fases

| Fase | Status |
|---|---|
| 0 — Reconhecimento | ✅ fechada (a pendência era o disassembly, agora resolvida) |
| 0b — Dump de `.text` | ✅ o dump existe e é código válido |
| 1 — Toolchain e DLL | ✅ DLL carrega, loga e escreve |
| 2 — Validar layout do Bag | parcial: tamanho de 64 bytes confirmado indiretamente; conteúdo ainda não |
| 3 — Xrefs | não iniciada |
| 4 — Decisão de arquitetura | **decidida: Opção A**, ver seção 3 |

---

## 6. Pendência: travamento ao carregar save

Ao dar load, o jogo travou. O Windows registrou `AppHangB1` para `re0hd.exe`,
ou seja **travamento, não access violation**.

O mod não instala nenhum hook nesta fase — só lê memória e faz polling de
teclas. Além disso, o log do mod termina normalmente, e o travamento veio
depois.

Indício adicional: o `re0box.log` da sessão anterior registra uma exceção
`E06D7363` logo após instalar os hooks dele. Dois mods diferentes falhando no
mesmo ponto aponta para causa anterior aos dois.

Teste em andamento: rodar com os dois `.asi` desativados. Se travar assim
também, o problema não é do mod.
