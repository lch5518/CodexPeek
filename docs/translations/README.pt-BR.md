# CodexPeek – Codex Usage Monitor for Windows

**Languages:** [English (default)](../../README.md) · [한국어](README.ko.md) · [Español](README.es.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [日本語](README.ja.md) · [हिन्दी](README.hi.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [Tiếng Việt](README.vi.md) · [Türkçe](README.tr.md) · [العربية](README.ar.md)

Codex Usage Monitor é um pequeno widget nativo para Windows que permite verificar seu uso do Codex rapidamente.
Ele mostra as janelas de limite de uso primária e secundária na barra de tarefas, em um widget flutuante e na bandeja do sistema.

![Widget do Codex Usage Monitor na barra de tarefas](../images/taskbar-widget-en.png)

## Destaques

- Mostra as janelas de uso primária e secundária do Codex, incluindo os horários de redefinição.
- Usa a interface `app-server` do Codex CLI instalado em vez de analisar arquivos de autenticação.
- Permite escolher manualmente entre até oito perfis de uso isolados.
- Permite mostrar o widget em todas as barras de tarefas ou apenas no monitor principal.
- Recorre com segurança a um widget flutuante e a um ícone de bandeja quando a fixação na barra de tarefas não está disponível.
- Oferece atualização manual, intervalos de atualização automática, inicialização com o Windows, diagnósticos e interface localizada.

## Como funciona

O monitor inicia `codex app-server --stdio` como um processo filho local e troca mensagens JSONL pela entrada e saída padrão.
O Codex CLI instalado gerencia sua própria autenticação e pode entrar em contato com a OpenAI conforme sua configuração e política de rede existentes.

O monitor solicita apenas o estado de sessão iniciada e as janelas de uso necessárias para exibição.
Ele não inicia uma tarefa do Codex nem chama `codex exec`.

## Perfis de uso

O perfil do sistema **Conta padrão do Codex**, que não pode ser excluído, usa o diretório inicial do
Codex herdado ao iniciar o CodexPeek ou o padrão da CLI quando `CODEX_HOME` não está
definido. Cada perfil gerenciado usa um diretório inicial do Codex separado em
`%APPDATA%\CodexPeek\profiles`. São permitidos oito perfis no total, incluindo o
perfil do sistema.

Os rótulos dos perfis são fornecidos por você. O CodexPeek não inspeciona e-mail nem ID
da conta; ao adicionar ou entrar novamente, confirme no navegador a conta do ChatGPT que
será usada. A seleção altera somente o uso que o CodexPeek consulta e exibe. Ela não muda
o login no terminal, IDE, aplicativo Codex, WSL, Remote SSH ou Dev Containers.

A seleção é sempre manual. O CodexPeek não seleciona nem alterna perfis automaticamente
conforme o limite restante e não encaminha trabalhos do Codex por um perfil. Excluir um
perfil gerenciado remove permanentemente os dados locais dele, inclusive as credenciais
da CLI armazenadas separadamente; leia a confirmação com atenção.

O CodexPeek nunca lê, analisa nem copia o `auth.json` de qualquer perfil. Somente o processo
filho `app-server` do perfil gerenciado recebe seu `CODEX_HOME` e a configuração de
armazenamento de credenciais em arquivo. Os diagnósticos incluem apenas contagens
agregadas, sem rótulos, caminhos ou dados de conta.

## Requisitos

- Windows 10 ou Windows 11, x64.
- Um [Codex CLI](https://github.com/openai/codex) com sessão iniciada e suporte a `account/read` e `account/rateLimits/read`.

## Baixar e executar

Primeiro, verifique se o Codex CLI está instalado e com sessão iniciada:

```powershell
codex --version
codex login status
```

### Instalador (recomendado)

1. Baixe `CodexPeek-Setup-v<version>-x64.exe` na
   [GitHub Release mais recente](https://github.com/lch5518/CodexPeek/releases/latest).
2. Execute o instalador e siga as instruções. Acesso de administrador não é necessário.
3. Inicie **Codex Usage Monitor** pelo menu Iniciar.

### Portable

1. Baixe `codex-peek-v<version>-windows-x86_64-portable.zip` na
   release mais recente.
2. Extraia o ZIP completamente em uma pasta gravável.
3. Execute `codex-peek.exe` na pasta extraída.

### Compilar a partir do código-fonte

Esta opção exige Rust 1.85 ou posterior, Visual Studio 2022 C++ Build Tools e um
Windows SDK. Ela executa o app a partir do repositório clonado e não cria um atalho no
menu Iniciar nem um desinstalador.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

Para verificar a compilação e a conexão com o Codex CLI sem abrir a interface:

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Pedir ao Codex para instalar

Copie o prompt abaixo no Codex. Ele prefere o Instalador verificado e só recorre a uma
compilação a partir do código-fonte quando assets de Release compatíveis não estão disponíveis.

```text
Instale o CodexPeek neste computador Windows x64 e conclua a verificação para mim.

1. Confirme que este é um Windows x64 e depois execute `codex --version` e `codex login status`.
2. Use apenas o repositório oficial e suas Releases:
   https://github.com/lch5518/CodexPeek
3. Prefira o `CodexPeek-Setup-v<version>-x64.exe` mais recente. Baixe-o junto com
   `SHA256SUMS.txt`, encontre a entrada exata do Instalador nesse arquivo, calcule o
   SHA-256 do Instalador e continue apenas se os hashes corresponderem. Não desative
   controles de segurança nem execute um arquivo cuja soma de verificação esteja ausente ou diferente.
4. Instale para o usuário atual sem solicitar acesso de administrador. Preserve as
   configurações existentes do CodexPeek e não interrompa um app em execução nem processos
   não relacionados; avise-me se eu precisar fechar o app por conta própria.
5. Somente se assets de Release compatíveis não estiverem disponíveis, clone o repositório oficial
   em um novo diretório gravável pelo usuário e execute `cargo build --release`. Se Git, Rust
   1.85+, Visual Studio 2022 C++ Build Tools ou um Windows SDK precisarem ser instalados, primeiro
   explique exatamente o que vai mudar e peça minha aprovação.
6. Nunca leia nem imprima o conteúdo de `%USERPROFILE%\.codex\auth.json`. A autenticação
   deve ser tratada somente pelo Codex CLI instalado.
7. Após a instalação ou compilação, execute o `codex-peek.exe --diagnose` resultante. Se ele
   for concluído com sucesso, inicie o CodexPeek.
8. Informe o método de instalação selecionado, a versão instalada, o local do executável,
   o resultado da soma de verificação e o resultado do diagnóstico. Se algo falhar, pare
   com segurança e explique o bloqueio exato sem expor informações sensíveis.
```

As edições Instalador e Portable usam `%APPDATA%\CodexPeek\settings.json`, então
as configurações são compartilhadas se você alternar entre elas. O instalador adiciona um
atalho no menu Iniciar, mas não habilita a inicialização com o Windows por padrão.

As primeiras releases não são assinadas por código e podem acionar o Microsoft Defender SmartScreen.
Baixe somente da release oficial e verifique o arquivo com `SHA256SUMS.txt`.

Consulte o [guia de instalação detalhado (em coreano)](../INSTALL.md) para verificação de hash,
atualizações, comportamento de desinstalação, diagnósticos e solução de problemas.

## Usar o monitor

Use o menu da bandeja para atualizar o uso, escolher um intervalo de atualização de 1/5/10/15/30 minutos e mostrar ou ocultar o widget.
Ele também oferece configurações de inicialização com o Windows, visualização inicial, atualização de autenticação, atualização automática de autenticação, idioma e diagnósticos.
Escolha **Widget: all monitors** ou **Widget: primary monitor only** para controlar o posicionamento em vários monitores; a seleção é lembrada entre reinicializações.

Por padrão, o idioma da interface segue a localidade do Windows quando ela corresponde a um idioma compatível. Você também pode escolher um idioma manualmente pelo menu da bandeja. Os idiomas compatíveis são coreano, inglês, espanhol, português brasileiro, indonésio, japonês, hindi, alemão, francês, vietnamita, turco e árabe.

O widget da barra de tarefas usa o tema claro/escuro do sistema Windows para o texto e deixa o material nativo da barra de tarefas aparecer através do fundo.

Apenas uma solicitação de uso é executada por vez. Solicitações com falha são repetidas com atrasos crescentes enquanto os últimos valores bem-sucedidos permanecem visíveis.

Se o widget da barra de tarefas não puder ser fixado após uma reinicialização do Explorer ou uma alteração no layout da barra de tarefas, o ícone da bandeja permanecerá disponível e o monitor tentará novamente com segurança.

## Privacidade e segurança

O monitor nunca lê nem analisa o conteúdo de `%USERPROFILE%\.codex\auth.json`.
Os diagnósticos verificam apenas se esse caminho existe.

Respostas RPC brutas são processadas apenas pelo tempo necessário para extrair o tipo de login e os campos de limite de uso exibidos.
Tokens, IDs de conta, endereços de e-mail, conteúdo de arquivos de autenticação e valores de proxy não são armazenados nem gravados em logs.

As configurações são armazenadas em `%APPDATA%\CodexPeek\settings.json`.
Um log de diagnóstico limitado é armazenado em `%TEMP%\codex-peek.log`.

Para a orientação completa sobre tratamento de dados e relato de vulnerabilidades, consulte [SECURITY.md](../../SECURITY.md).

## Solução de problemas

| Problema | O que fazer |
| --- | --- |
| Codex CLI não foi encontrado | Execute `codex --version` e `where.exe codex`, depois confira se o Codex CLI está no `PATH`. |
| O CLI não é compatível | Atualize o Codex CLI. O suporte aos RPCs exigidos é mais importante que o número de versão exibido. |
| Sessão encerrada ou autenticação expirada | Conclua o fluxo normal de login no Codex CLI e depois escolha **Refresh authentication** no menu da bandeja. |
| O widget da barra de tarefas está no monitor errado | Escolha **Widget: all monitors** ou **Widget: primary monitor only** no menu da bandeja. |
| O widget da barra de tarefas sumiu | Use o widget flutuante ou o ícone da bandeja, reinicie o Explorer se necessário e selecione o modo de monitor do widget preferido. |
| Mais detalhes são necessários | Execute `--diagnose` ou abra **Diagnostics** pelo menu da bandeja. |

## Desenvolvimento

Compilações a partir do código-fonte exigem Rust 1.85 ou posterior, Visual Studio 2022 C++ Build Tools e um
Windows SDK. Compile e valide a partir da raiz do repositório:

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

As verificações automatizadas não substituem os cenários de recuperação de Windows, DPI, vários monitores e Explorer na [lista de verificação de release](../RELEASE_CHECKLIST.md).

## ❤️ Apoio

Se o CodexPeek economiza seu tempo, considere apoiar seu desenvolvimento.

- ⭐ Dê uma estrela a este repositório
- ❤️ [Patrocinar no GitHub](https://github.com/sponsors/lch5518)

Cada patrocínio ajuda a manter o projeto ativo.

## Licença

Este projeto está disponível sob a [MIT License](../../LICENSE).
Consulte [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) para avisos de terceiros.
