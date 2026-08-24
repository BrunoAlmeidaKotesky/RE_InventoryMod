---
name: re0-phase-wrapup
description: Encerra uma fase do projeto RE0 Inventory Expansion. Use ao concluir qualquer fase do plano, quando o usuario pedir documentacao de fase, ou quando pedir o documento de buildup para aprender o que foi feito. Produz o doc publico em ingles, o doc de aprendizado em portugues, e o commit.
---

# Encerramento de fase

Toda fase concluida gera tres entregas. Nao pular nenhuma.

## 1. Documentacao publica — `docs/`, em ingles

Descreve o **jogo** e o **projeto**, nunca a maquina do usuario.

- Ingles, sempre.
- Sem caminho local (`D:\SteamLibrary\...`), sem id de usuario Steam, sem nome
  de pasta pessoal.
- Fato verificado apenas. Todo endereco vem acompanhado de como foi
  confirmado.
- Atualizar `docs/game-internals.md` com o que se descobriu do jogo;
  `docs/building.md` se o processo de build mudou.

## 2. Documento de buildup — `dev-notes/learning/`, em portugues

O usuario nao e desenvolvedor Rust e nao conhece engenharia reversa nem
gerenciamento de memoria nesse nivel. Este documento existe para ele aprender
**depois** que tudo funcionar.

Nome: `dev-notes/learning/fase-N-<assunto>.md`.

Estrutura:

1. **O que essa fase resolveu** — o problema, em uma frase
2. **Os conceitos novos** — cada conceito que apareceu, explicado do zero:
   o que e, por que existe, o que acontece sem ele. Ex.: loader lock, thiscall,
   secao de PE, import library, janela deslizante
3. **O codigo, linha a linha** — os trechos que importam, explicando por que
   cada decisao foi tomada e o que quebraria na alternativa
4. **O que deu errado no caminho** — inclusive os erros meus. E onde mais se
   aprende
5. **Como verificar sozinho** — comandos que o usuario pode rodar para ver o
   mesmo resultado

Escrever para quem nunca viu o assunto. Nada de "como sabemos" ou "trivialmente".

## 3. Notas internas — `dev-notes/`, em portugues

`dev-notes/NN-<fase>.md` com o registro cru: o que foi tentado, o que falhou,
enderecos, saidas de comando, hipoteses descartadas. Pode conter caminho local
— nao vai para o repositorio publico.

## 4. Commit

Mensagem em portugues, corpo explicando **por que**, nao so o que. Um assunto
por commit.

## Checklist

- [ ] `cargo clippy --all-targets` limpo
- [ ] `docs/` atualizado, em ingles, sem caminho local
- [ ] `dev-notes/learning/fase-N-*.md` escrito
- [ ] `dev-notes/NN-*.md` atualizado
- [ ] Pasta do jogo restaurada (`tools\uninstall.ps1`) se houve teste
- [ ] Commit feito
