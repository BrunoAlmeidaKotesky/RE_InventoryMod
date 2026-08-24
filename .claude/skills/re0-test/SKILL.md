---
name: re0-test
description: Protocolo obrigatorio para qualquer teste do mod dentro do jogo. Use sempre que for instalar, testar in-game, pedir para o usuario rodar o jogo, ou diagnosticar travamento/crash. Cobre backup, uma variavel por teste, e restauracao.
---

# Teste in-game do RE0 Inventory Expansion

A instalacao do jogo e do usuario, nao um ambiente descartavel. Ela funciona
hoje com o re0box instalado. Qualquer coisa que quebre depois de uma mudanca
nossa e responsabilidade nossa.

## Regras

1. **Nunca editar a pasta do jogo na mao.** Nem copiar, nem renomear, nem
   apagar. Tudo passa por `tools\install.ps1` e `tools\uninstall.ps1`, que
   registram e desfazem via manifesto.

2. **Uma variavel por rodada.** Se o teste mudou duas coisas, o resultado nao
   diz qual delas importou.

3. **Nao desativar outros plugins ASI sem necessidade comprovada.** Exige a
   flag `-DisableOtherAsi`. Desativar o re0box na mao ja deixou a instalacao
   travando no load, sem caminho de volta.

4. **Devolver a maquina limpa.** Ao fim de qualquer sessao de teste, ou assim
   que um teste falha, rodar `uninstall.ps1` antes de continuar a conversa.

5. **Nunca atribuir a falha a causa pre-existente.** O baseline funciona. Se
   nao houver mecanismo comprovado, dizer exatamente isso: "nao tenho mecanismo
   comprovado". Nao construir teoria de que o problema ja existia.

## Antes de pedir para o usuario rodar

- [ ] `cargo clippy --all-targets` limpo
- [ ] Build feito com `tools\build.ps1`
- [ ] `tools\install.ps1` rodado (ele ja faz backup do save)
- [ ] Saber exatamente qual pergunta este teste responde
- [ ] Saber o que fazer com cada resultado possivel, antes de pedir

## Depois de um teste

Ler, nesta ordem:

1. `re0inv.log` na pasta do jogo — o que o mod viu
2. Log de eventos do Windows, aplicacao `re0hd.exe` — `APPCRASH` (excecao) ou
   `AppHangB1` (travou, deixou de responder)
3. Hash do save contra o backup mais recente, para confirmar que nao mudou

## Diagnostico de travamento

`AppHangB1` significa que a janela parou de responder, nao que houve acesso
invalido. Perguntar ao usuario: fechou sozinho ou ele matou o processo, e
quanto tempo esperou.

Comecar sempre pela hipotese de que o mod causou. Verificar, nesta ordem:
o que a thread do mod estava fazendo; se algum hook estava instalado; o que o
install script deixou na pasta.
