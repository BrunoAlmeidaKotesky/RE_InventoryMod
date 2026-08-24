---
name: re0-memory-safety
description: Regras de seguranca de memoria para o mod do RE0. Use antes de escrever qualquer codigo unsafe, ponteiro cru, leitura ou escrita na memoria do jogo, hook, trampolim ou patch de bytes. Tambem ao revisar codigo que ja faz isso.
---

# Seguranca de memoria neste mod

Este codigo roda dentro do processo do jogo. Nao existe sandbox: um ponteiro
errado nao devolve `Err`, derruba o jogo do usuario. Pior, pode corromper
estado que so aparece minutos depois, em outro lugar.

## Ler memoria

**Nunca desreferenciar um endereco que nao foi provado valido.** Use
`debug::memory::read`, que passa por `ReadProcessMemory` no proprio processo:
uma pagina desmapeada ou protegida devolve `false` em vez de gerar falha.

Desreferenciar direto so e aceito quando o endereco veio do cabecalho PE do
proprio modulo, e mesmo assim dentro de `unsafe fn` com comentario `# Safety`
explicando a garantia.

Enderecos de terceiros — tabela do re0box, palpite, resultado de scan — sao
hipoteses ate serem verificados nesta build.

## Escrever memoria

Ainda nao ha escrita neste projeto. Quando houver:

- Trocar a protecao com `VirtualProtect`, escrever, **restaurar a protecao
  original**. Deixar pagina de codigo com `PAGE_EXECUTE_READWRITE` e superficie
  de ataque e mascara bug.
- Escrever com o jogo em estado conhecido. Patchear codigo que outra thread
  pode estar executando naquele instante corrompe o pipeline.
- Guardar os bytes originais antes de sobrescrever. Sem isso nao ha como
  desinstalar o hook nem diagnosticar.
- Um hook por commit. Quando quebrar, o bisect precisa ser trivial.

## Fronteira FFI

- **Panico nao pode atravessar `extern "system"`.** E comportamento indefinido.
  Toda funcao chamada pelo jogo ou por thread nossa precisa de
  `catch_unwind` na raiz.
- `DllMain` roda segurando o loader lock. Nada de I/O, alocacao pesada, espera
  ou `LoadLibrary` la dentro. Apenas criar thread e retornar.
- `unwrap()` em codigo que roda dentro do jogo e um crash disfarcado. Preferir
  `let ... else` com log e retorno.

## Convencoes do codigo do jogo

O jogo e C++ compilado com MSVC. Ao hookear:

- `__thiscall`: `this` vem em `ecx`, argumentos na pilha, callee limpa a pilha
  (`ret N`).
- Patch no meio de funcao precisa preservar **todos** os registradores que a
  funcao original considera vivos, inclusive flags. `pushad`/`popad` e
  `pushfd`/`popfd` quando houver duvida.
- Verificar o tamanho das instrucoes sobrescritas. Um `jmp` de 5 bytes por cima
  de uma instrucao de 3 bytes corta a seguinte ao meio.

## Layout de struct

`#[repr(C)]` em toda struct que espelha memoria do jogo, sempre. Sem ele o Rust
pode reordenar campos.

Confirmar tamanho com `assert!` em tempo de compilacao:

```rust
const _: () = assert!(std::mem::size_of::<Bag>() == 64);
```

## Antes de qualquer patch

1. Desassemblar o endereco no dump e confirmar que o codigo faz o esperado
2. Conferir que a build bate com a suportada
3. Preferir assinatura de bytes a endereco literal
4. Registrar em `docs/game-internals.md` como foi confirmado
