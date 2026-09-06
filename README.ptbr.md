# RE0 QoL

*In English: [README.md](README.md)*

Três mods pequenos para **Resident Evil 0 HD Remaster** na Steam. Instale
os três ou só o que você quiser.

- **Inventário expandido**: 12 slots por personagem em vez de 6. O painel
  continua mostrando 6 de cada vez, você rola para ver o resto.
- **Baú de itens**: toda máquina de escrever tem um baú, como nos outros
  jogos da série.
- **Portas instantâneas**: tira a animação de porta entre as salas.

Nada na pasta do jogo é substituído. Os slots extras e o baú ficam salvos
num arquivo separado ao lado do jogo (`re0inv_saves.bin`) e os saves do jogo
não são tocados. No pior caso, se o mod quebrar, você perde o que estava
nos slots extras ou no baú, não o seu progresso.

Isto é um beta. Só testei na minha máquina. Faça backup dos saves antes de
experimentar, eles ficam em `...\Steam\userdata\<seu id>\339340\remote`.

## Requisitos

- Resident Evil 0 HD Remaster na Steam, build atual (28 de janeiro de
  2025). Em qualquer outra build o mod carrega, escreve uma linha no
  `re0inv.log` avisando, e não muda nada.
- Mais nada. O Ultimate ASI Loader (`dinput8.dll`) já vem no zip.

## Instalar

Baixe um zip da
[release mais recente](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/releases/latest):

| Zip | O que tem dentro |
|---|---|
| `RE0-QoL-Bundle` | Os três. Cada um pode ser desligado no `re0inv.ini`. |
| `RE0-InventoryExpansion` | Só os 12 slots |
| `RE0-ItemBox` | Só o baú |
| `RE0-DoorSkip` | Só as portas instantâneas |

Extraia na pasta do jogo, a que tem o `re0hd.exe` (normalmente
`C:\Program Files (x86)\Steam\steamapps\common\Resident Evil 0`). Só isso.

Se já existir um `dinput8.dll` de outro mod, fique com qualquer um, é o
mesmo carregador. O "verificar integridade dos arquivos" da Steam não vai
reclamar, nenhum arquivo original é mexido.

## Como funciona no jogo

**Rolar o inventário.** Com o cursor na última fileira, aperte para baixo
mais uma vez e o painel rola. Clicar o analógico direito, ou Page Up / Page
Down no teclado, também rola. No fim ele volta pro começo.

**Baú de itens.** Use uma máquina de escrever e aparece uma opção nova na
mensagem dela. O baú abre onde normalmente fica o inventário do parceiro, e
você move as coisas com o Exchange, nos dois sentidos. Se estiver do lado de
uma máquina com o inventário aberto, Home (ou clicar o analógico esquerdo)
também abre. Ele fecha junto com o inventário.

**Combinar.** A fileira do primeiro item que você escolheu fica na tela
enquanto o resto rola, então o segundo item pode estar em qualquer página.

**Configurações** ficam no `re0inv.ini`. Cada opção tem um comentário do
lado. `Mod=0` desliga tudo sem desinstalar.

## Deu problema?

Olhe primeiro o `re0inv.log` na pasta do jogo.

Se o jogo travar, não feche na hora. Dê uns 15 segundos: o mod percebe o
travamento e grava `re0inv_hang.dmp` e `re0inv_hang.txt` ao lado do jogo.
Sem eles não tenho como saber onde travou. Abra uma
[issue](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/issues) e
anexe os dois junto com o log.

## Desinstalar

Antes de tirar o mod, mova o que você quer guardar para os seis primeiros
slots. Os slots extras e o baú não fazem parte do save do jogo, então o jogo
não vai enxergar eles depois que o mod sair.

Depois apague da pasta do jogo:

- `scripts\re0inv.asi`
- `re0inv.ini`, `re0inv.log`, `re0inv_hang.dmp`, `re0inv_hang.txt`
- `nativePC\arc\message\msg_*_inv.arc` (o texto da máquina de escrever que
  o mod gerou)
- `dinput8.dll`, a menos que outro mod use ele

O `re0inv_saves.bin` guarda os itens do mod. Mantenha se pretende
reinstalar.

## Limitações conhecidas

- Não funciona junto com o [re0box](https://github.com/descawed/re0box).
  Os dois mexem na mesma parte do jogo, escolha um.
- Só uma variante do zip por vez. Instalar uma segunda por cima substitui a
  primeira.

## Compilar do código

Rust, target GNU de 32 bits:

```
rustup target add i686-pc-windows-gnu
tools\build.ps1
```

O `tools\release.ps1` gera os quatro zips. As anotações sobre o interior do
jogo (endereços, estruturas, o que a engenharia reversa achou) estão em
[docs/](docs/).

## Créditos

- [Ultimate ASI Loader](https://github.com/ThirteenAG/Ultimate-ASI-Loader)
  do ThirteenAG, vai sem modificação em cada zip sob a licença MIT dele
  (`LICENSE-Ultimate-ASI-Loader.txt`).
- [re0box](https://github.com/descawed/re0box) do descawed, o primeiro mod
  de baú para este jogo. Foi minha referência de como o jogo lida com o
  inventário. Nenhum código dele é usado aqui; ele não tem licença, então
  todos os direitos ficam com o autor.

As releases não contêm nenhum arquivo do jogo. O mod lê o que precisa da sua
própria instalação. Não mexe no DRM da Steam e precisa de uma cópia
legítima. Resident Evil é marca da Capcom e este projeto não tem ligação com
a Capcom.

Código sob a [licença MIT](LICENSE).
