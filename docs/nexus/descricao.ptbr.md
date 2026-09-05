# Texto da página no Nexus Mods (versão em português)

O Nexus é em inglês; o texto que vai para a página é o de
[description.md](description.md). Esta é a mesma coisa em português, para
conferir o que está sendo dito, ou para postar como tradução se quiser.

---

[size=5][b]Doze slots de inventário, um baú em toda máquina de escrever, e portas que abrem na hora.[/b][/size]

Três melhorias de qualidade de vida para Resident Evil 0 HD Remaster, num download só. Instala copiando arquivos para a pasta do jogo. Nada para rodar, nenhum arquivo do jogo substituído, e o "verificar integridade" da Steam continua limpo.

[size=4][b]O que faz[/b][/size]

[list]
[*][b]Inventário expandido[/b] - 12 slots por personagem em vez de 6. Os slots extras rolam dentro do mesmo painel que o jogo já desenha: com o cursor na última fileira, aperte para baixo de novo e a próxima fileira entra. O clique do analógico direito e Page Up / Page Down também rolam.
[*][b]Baú de itens[/b] - o baú que o resto da série tem, em toda máquina de escrever. A própria mensagem da máquina oferece ele. Mova itens com o Exchange nos dois sentidos; o baú guarda 24 itens e rola como o inventário.
[*][b]Portas instantâneas[/b] - a animação de porta entre salas some. A sala muda em bem menos de um segundo, sem nada carregado antes da hora ou pulado.
[/list]

Seus itens extras e o conteúdo do baú sobrevivem a salvar, sair e carregar. Ficam num arquivo pequeno ao lado do jogo e [b]o save do jogo nunca é escrito[/b], então uma falha só pode custar o que o mod adicionou, nunca o seu progresso.

Combinar funciona entre páginas: a fileira do primeiro item fica na tela enquanto as outras rolam, então o segundo item pode estar em qualquer lugar.

[size=4][b]Instalar[/b][/size]

[list=1]
[*]Copie o conteúdo do zip para a pasta do jogo (a que tem o re0hd.exe, normalmente ...\steamapps\common\Resident Evil 0).
[*]Você também precisa do [url=https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases]Ultimate ASI Loader[/url]: baixe o dinput8.dll dele (build Win32) e coloque na mesma pasta. Pule se outro mod já te deu um.
[/list]

Escolha [b]um[/b] pacote. O arquivo principal tem as três melhorias, cada uma chaveável no re0inv.ini; os opcionais têm uma cada. Eles se substituem.

Funciona com a build atual da Steam (28 jan 2025). Em qualquer outra o mod carrega, anota no re0inv.log, e não muda nada.

[size=4][b]Desinstalar[/b][/size]

Apague scripts\re0inv.asi, re0inv.ini, re0inv.log, qualquer re0inv_hang.* e nativePC\arc\message\msg_*_inv.arc da pasta do jogo. O re0inv_saves.bin guarda os itens que o mod salvou para você: mantenha se pretende reinstalar. Mova o que importa para os seis primeiros slots antes, já que os slots extras e o baú não estão no save do jogo.

[size=4][b]Bom saber[/b][/size]

[list]
[*]Beta. Testado numa única máquina. Faça backup do save antes de experimentar.
[*]Não é compatível com o re0box: os dois mexem na mesma parte do jogo.
[*]Se o jogo travar, espere uns quinze segundos antes de fechar. O mod grava re0inv_hang.dmp e re0inv_hang.txt ao lado do jogo; poste esses com o re0inv.log e o travamento pode ser consertado.
[*]Código-fonte e documentação de tudo que foi descoberto sobre o jogo: [url=https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod]GitHub[/url]. Relatos de bug vão lá também.
[/list]

[size=4][b]Créditos[/b][/size]

O re0box, de descawed, foi a referência técnica para o sistema de inventário do jogo; nenhum código dele é usado. Este mod não contém assets da Capcom e não toca no DRM da Steam. Resident Evil 0 é marca registrada da Capcom; este projeto não é afiliado à Capcom.

---

## Descrições dos arquivos (uma linha cada, para a aba Files)

- **RE0-QoL-Bundle** (arquivo principal): As três: 12 slots de inventário, baú em toda máquina de escrever, portas instantâneas. Cada uma pode ser desligada no re0inv.ini.
- **RE0-InventoryExpansion** (opcional): 12 slots de inventário por personagem em vez de 6, rolados dentro do painel do próprio jogo. Só isso.
- **RE0-ItemBox** (opcional): Um baú de itens em toda máquina de escrever, oferecido na mensagem da própria máquina. Só isso.
- **RE0-DoorSkip** (opcional): A animação de porta entre salas removida. Só isso.

## Descrição curta (o campo de resumo de uma linha)

Doze slots de inventário, um baú em toda máquina de escrever, e portas instantâneas. Copie para a pasta do jogo; nenhum arquivo do jogo substituído.
