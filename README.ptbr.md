# RE0 QoL — Inventário Expandido, Baú de Itens, Portas Instantâneas

*Read this in [English](README.md).*

Três melhorias de qualidade de vida para **Resident Evil 0 HD Remaster** no PC
(Steam). Instala copiando arquivos para a pasta do jogo. Nenhuma ferramenta
para rodar, nenhum arquivo do jogo substituído.

- **Inventário expandido** — 12 slots por personagem em vez de 6. Os slots
  extras rolam dentro do mesmo painel que o jogo já desenha.
- **Baú de itens** — o baú que o resto da série tem, em toda máquina de
  escrever. A própria mensagem da máquina oferece ele.
- **Portas instantâneas** — a animação de atravessar a porta some. A sala
  muda em bem menos de um segundo.

Seus itens extras e o conteúdo do baú sobrevivem a salvar, sair e carregar.
Ficam num arquivo pequeno ao lado do jogo, e **o save do jogo nunca é
tocado**: o pior que qualquer falha poderia custar é o que o mod adicionou,
nunca o seu progresso.

> **Status: beta.** Testado numa única máquina. Faça backup do seu save antes
> de experimentar (ele fica em
> `%ProgramFiles(x86)%\Steam\userdata\<seu id>\339340\remote`).

## Download

Pegue **um** zip na [release mais recente](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/releases/latest).
Eles se substituem, então escolha o que tem o que você quer:

| Pacote | O que vem |
|---|---|
| `RE0-QoL-Bundle` | As três. Cada uma pode ser desligada no `re0inv.ini` |
| `RE0-InventoryExpansion` | Só os 12 slots |
| `RE0-ItemBox` | Só o baú de itens |
| `RE0-DoorSkip` | Só as portas instantâneas |

## Instalar

1. Copie o conteúdo do zip para a pasta do jogo — a que contém o `re0hd.exe`,
   normalmente `C:\...\steamapps\common\Resident Evil 0`.
2. Você também precisa do [Ultimate ASI Loader](https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases):
   baixe o `dinput8.dll` dele (a build Win32) e coloque na mesma pasta. Pule
   se já tiver por causa de outro mod.

É só isso. Na primeira vez que o jogo abre, o mod monta o texto da máquina de
escrever a partir dos seus próprios arquivos; os originais nunca são
modificados, então "verificar integridade" na Steam não acusa nada.

Funciona com a build atual da Steam (`28 jan 2025`). Em qualquer outra, o mod
carrega, anota no `re0inv.log`, e não muda nada.

## Como usar

**Rolar o inventário.** Com o cursor na última fileira, aperte para baixo de
novo: a próxima fileira de itens entra. Continue apertando para chegar em
todos os slots; no fim volta para o começo. Clicar o analógico direito faz o
mesmo de qualquer lugar do painel, assim como Page Up / Page Down no teclado.
Apertar para cima na primeira fileira continua indo para as abas, como sempre.

**O baú de itens.** Use uma máquina de escrever. A mensagem dela agora tem uma
opção a mais, que abre o inventário com o baú na metade do parceiro. Mova
coisas com o Exchange, nos dois sentidos: seu item para o baú, ou um item do
baú para qualquer slot seu, ocupado ou não (o Exchange troca os dois). O baú
rola com as mesmas teclas enquanto sua seleção está dentro dele. Perto de uma
máquina você também pode abrir direto do inventário com Home ou um clique no
analógico esquerdo. Ele fecha quando você sai do inventário.

**Combinar entre páginas.** Escolha o primeiro item e Combine. A fileira em
que ele está fica parada na tela enquanto as outras duas rolam, então o
segundo item pode estar em qualquer página.

**Configurações** ficam no `re0inv.ini` na pasta do jogo, com um comentário em
cada linha. `Mod=0` desliga tudo sem desinstalar.

## Se algo der errado

- O `re0inv.log` na pasta do jogo diz o que o mod fez. Anexe num relato.
- Se o jogo travar, **espere uns quinze segundos antes de fechar**. O mod
  percebe e grava `re0inv_hang.dmp` e `re0inv_hang.txt` ao lado do jogo;
  esses dois arquivos são o que torna um travamento consertável. Apague depois
  de enviar (o `.dmp` é grande).
- Relate problemas na [página de issues](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/issues).

## Desinstalar

Apague `scripts\re0inv.asi`, `re0inv.ini`, `re0inv.log` e qualquer
`re0inv_hang.*` da pasta do jogo, mais
`nativePC\arc\message\msg_*_inv.arc`. O `re0inv_saves.bin` guarda os itens que
o mod salvou para você: mantenha se pretende reinstalar, apague caso
contrário.

Itens nos slots extras ou no baú não estão no save do jogo, então mova o que
importa para os seis primeiros slots antes de desinstalar.

## Bom saber

- Só um pacote pode estar instalado por vez; instalar outro substitui.
- Este mod e o [re0box](https://github.com/descawed/re0box) mexem na mesma
  parte do jogo e não podem rodar juntos.
- Combine e Exchange trabalham com o que está na tela, uma página por vez; a
  fileira do primeiro item fica presa para o segundo poder vir de qualquer
  lugar.

## Para desenvolvedores

O código é Rust, compilado para o target GNU de 32 bits
(`rustup target add i686-pc-windows-gnu`, depois `tools\build.ps1`). Tudo que
foi descoberto sobre o interior do jogo está em [docs/](docs/), e
`tools\release.ps1` monta os quatro pacotes.

## Créditos e questões legais

O **re0box, de descawed**, é a melhor documentação existente do sistema de
inventário deste jogo, e este projeto se apoia nele como referência técnica:
endereços de função, layout de dados, comportamento observado. Isso são fatos
sobre o jogo, não autoria. O repositório do re0box não tem arquivo de licença,
o que significa todos os direitos reservados ao autor; **nenhum código do
re0box foi copiado para este projeto**.

Este repositório e suas releases **não contêm nenhum asset da Capcom** — nem
executável, nem arquivos, nem modelos, nem saves. Tudo que o mod precisa do
jogo, ele lê da sua própria instalação enquanto roda. Você precisa ter o jogo.

Este projeto não desativa, contorna nem interfere com o DRM da Steam ou
verificações de posse. Exige uma cópia legítima, autenticada na Steam.

Resident Evil 0 é marca registrada da Capcom. Este projeto não é afiliado nem
endossado pela Capcom.

## Licença

O código-fonte deste repositório é liberado sob a licença MIT. Veja
[LICENSE](LICENSE).
