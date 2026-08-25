# RE0 QoL — Inventário Expandido, Baú de Itens, Portas Instantâneas

*Read this in [English](README.md).*

Melhorias de qualidade de vida para **Resident Evil 0 HD Remaster** (PC /
Steam), escritas em Rust e distribuídas como plugin ASI de 32 bits.

- **Inventário expandido** — 12 slots por personagem em vez de 6, rolados
  dentro do próprio painel do jogo. Pressione para baixo além da última
  fileira, ou clique o analógico direito.
- **Baú de itens** — armazenamento em toda máquina de escrever, oferecido na
  própria mensagem da máquina, como no resto da série. Depositar é entregar o
  item ao painel do baú; retirar é pegar de volta.
- **Portas instantâneas** — a transição entre salas cortada de uns três
  segundos e meio para bem menos de um.

Tudo que o mod guarda — os slots extras e o baú — sobrevive a salvar e
carregar, mantido num arquivo próprio ao lado do jogo. **O save do jogo nunca
é escrito**, então o pior que qualquer falha pode custar é o que o mod
adicionou.

> **Status: beta.** Em fase de testes no jogo, numa única máquina. Faça backup
> do seu save antes de experimentar.

## Download

Cada melhoria também sai sozinha. Escolha **um** pacote — eles se substituem:

| Pacote | O que vem |
|---|---|
| `RE0-QoL-Bundle` | As três, cada uma chaveável no `re0inv.ini` |
| `RE0-InventoryExpansion` | Só os 12 slots |
| `RE0-ItemBox` | Só o baú de itens |
| `RE0-DoorSkip` | Só as portas instantâneas |

## Instalar

1. Copie o conteúdo do zip para a pasta do jogo — a que contém `re0hd.exe`,
   normalmente `...\steamapps\common\Resident Evil 0`.
2. Coloque o `dinput8.dll` (build Win32) do
   [Ultimate ASI Loader](https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases)
   na mesma pasta, se ainda não estiver lá.

Só isso. Na primeira vez que o jogo abrir, o mod gera os textos da máquina de
escrever a partir dos seus próprios arquivos; os originais nunca são
modificados, então verificar os arquivos na Steam não acusa nada.

Build suportada: `MasterRelease Jan 28 2025 16:45:59`. Em qualquer outra o mod
carrega, avisa no log, e não altera nada.

## Desinstalar

Apague `scripts\re0inv.asi`, `re0inv.ini`, `re0inv.log` e
`nativePC\arc\message\msg_*_inv.arc` da pasta do jogo. O `re0inv_saves.bin`
guarda os itens que o mod salvou para você — mantenha se pretende reinstalar.

## Compilando do código

O jogo é um processo de 32 bits, então o mod precisa de um target de 32 bits.
O target GNU é usado porque traz o próprio linker:

```sh
rustup target add i686-pc-windows-gnu
powershell -File tools\build.ps1          # uma DLL, todas as melhorias
powershell -File tools\release.ps1        # os quatro pacotes, em dist\
```

Cada melhoria é uma Cargo feature (`expanded`, `itembox`, `doors`), então uma
DLL de melhoria única é `cargo build --release -p re0inv
--no-default-features --features doors`, renomeada para `.asi`.

## Compatibilidade

Este mod intercepta o mesmo subsistema de inventário que o
[re0box](https://github.com/descawed/re0box). Os dois não podem rodar juntos;
trate como mutuamente exclusivos.

## Créditos e questões legais

O **re0box, de descawed**, é a melhor documentação existente do subsistema de
inventário deste jogo, e este projeto se apoia nele como referência técnica:
endereços de função, layout de structs e comportamento observado. Isso são
fatos sobre o jogo, não autoria. O repositório do re0box não tem arquivo
LICENSE, o que significa todos os direitos reservados ao autor; **nenhum
código do re0box foi copiado para este projeto**.

Este repositório e suas releases **não contêm nenhum asset da Capcom** — nem
executável, nem arquivos, nem modelos, nem saves. Tudo que o mod precisa do
jogo, ele lê da sua própria instalação em tempo de execução. Você precisa ter
o jogo.

Este projeto não desativa, contorna nem interfere com o DRM da Steam ou
verificações de posse. Exige uma cópia legítima, autenticada na Steam.

Resident Evil 0 é marca registrada da Capcom. Este projeto não é afiliado nem
endossado pela Capcom.

## Licença

O código-fonte deste repositório é liberado sob a licença MIT. Veja
[LICENSE](LICENSE).
