# RE0 QoL — Inventário Expandido, Baú de Itens, Portas Instantâneas

*Read this in [English](README.md).*

Três mods de qualidade de vida para **Resident Evil 0 HD Remaster** no PC
(Steam). Copie um zip para a pasta do jogo e jogue. Nada para rodar, nenhum
arquivo do jogo substituído.

- **Inventário expandido** — 12 slots por personagem em vez de 6, rolados
  dentro do painel do próprio jogo.
- **Baú de itens** — o baú que o resto da série tem, em toda máquina de
  escrever.
- **Portas instantâneas** — sem a animação de porta entre salas.

Itens extras e o baú sobrevivem a salvar e carregar. Ficam num arquivo
pequeno ao lado do jogo; **o save do jogo nunca é tocado**, então uma falha
só pode custar o que o mod adicionou.

> **Beta.** Testado numa única máquina. Faça backup do save antes
> (`%ProgramFiles(x86)%\Steam\userdata\<seu id>\339340\remote`).

## Baixar e instalar

Escolha **um** zip na [release mais recente](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/releases/latest):

| Pacote | O que vem |
|---|---|
| `RE0-QoL-Bundle` | As três, cada uma chaveável no `re0inv.ini` |
| `RE0-InventoryExpansion` | Só os 12 slots |
| `RE0-ItemBox` | Só o baú |
| `RE0-DoorSkip` | Só as portas instantâneas |

Copie o conteúdo dele para a pasta do jogo, a que tem o `re0hd.exe`
(normalmente `C:\...\steamapps\common\Resident Evil 0`). Pronto. O zip inclui
o `dinput8.dll`, o Ultimate ASI Loader que carrega o mod; se outro mod já
colocou um lá, tanto faz qual fica. O "verificar integridade" da Steam
continua limpo.

Funciona com a build atual da Steam (28 jan 2025). Em qualquer outra, o mod
carrega, anota no `re0inv.log`, e não muda nada.

## Como usar

- **Rolar:** na última fileira, aperte para baixo de novo. Clique do
  analógico direito e Page Up / Page Down também rolam. No fim volta ao
  começo.
- **Baú:** use uma máquina de escrever e escolha a opção nova na mensagem
  dela. Mova itens com o Exchange, nos dois sentidos. Perto de uma máquina,
  Home ou clique do analógico esquerdo também abre pelo inventário. Fecha
  junto com o inventário.
- **Combinar:** a fileira do primeiro item fica na tela enquanto o resto rola,
  então o segundo item pode estar em qualquer página.
- **Configurações:** `re0inv.ini` na pasta do jogo, um comentário por linha.
  `Mod=0` desliga tudo sem desinstalar.

## Se algo der errado

O `re0inv.log` na pasta do jogo diz o que o mod fez. Se o jogo travar,
**espere uns quinze segundos antes de fechar**: o mod grava `re0inv_hang.dmp`
e `re0inv_hang.txt` ao lado do jogo, e são eles que tornam um travamento
consertável. Mande junto com o log na
[página de issues](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/issues).

## Desinstalar

Antes, mova o que importa para os seis primeiros slots: os slots extras e o
baú não estão no save do jogo. Depois apague `scripts\re0inv.asi`,
`re0inv.ini`, `re0inv.log`, `re0inv_hang.*` e
`nativePC\arc\message\msg_*_inv.arc`; o `dinput8.dll` também, a menos que
outro mod use ele. O `re0inv_saves.bin` guarda os itens do mod, mantenha se
pretende reinstalar.

Não é compatível com o [re0box](https://github.com/descawed/re0box): os dois
mexem na mesma parte do jogo.

## Para desenvolvedores

Rust, compilado para o target GNU de 32 bits (`rustup target add
i686-pc-windows-gnu`, depois `tools\build.ps1`; `tools\release.ps1` monta os
quatro pacotes). O que foi descoberto sobre o interior do jogo está em
[docs/](docs/).

## Créditos e questões legais

O **Ultimate ASI Loader, de ThirteenAG**, vai dentro de todo pacote, sem
modificação, sob a licença MIT dele (incluída). O **re0box, de descawed**, foi
a referência técnica para o sistema de inventário do jogo: endereços, layouts,
comportamento observado, que são fatos sobre o jogo, não autoria. O
repositório do re0box não tem licença, então todos os direitos são do autor;
nenhum código dele foi copiado para cá.

Nenhum asset da Capcom está neste repositório ou nas releases; o mod lê o que
precisa da sua própria instalação enquanto roda. Não toca no DRM da Steam nem
em verificações de posse, e exige uma cópia legítima. Resident Evil 0 é marca
registrada da Capcom; este projeto não é afiliado à Capcom.

Código-fonte: licença MIT, veja [LICENSE](LICENSE).
