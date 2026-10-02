# Especificação Arquitetural: rcompose (Docker Compose para WSL Containers em Rust)

- **Data**: 2026-10-02
- **Status**: Aprovado
- **Autor**: Antigravity & Ricardo Borges
- **Caminho**: `docs/superpowers/specs/2026-10-02-rcompose-design.md`

---

## 1. Visão Geral

O **`rcompose`** é uma ferramenta de linha de comando de alto desempenho escrita em **Rust**, projetada para trazer a experiência e compatibilidade do **Docker Compose** para o novo ecossistema nativo de contêineres do Windows Subsystem for Linux (**WSLC** - `wslc.exe`).

Inspirado na usabilidade do `docker-compose`, na arquitetura distribuída e extensibilidade do `rancher-compose` e na implementação de referência em Python (`ref/wslc-compose-main`), o `rcompose` oferece:
- Execução como binário nativo Windows (`rcompose.exe`), com suporte transparente a interoperabilidade dentro do WSL.
- Trait modular `ContainerEngine` com backend assíncrono inicial via CLI `wslc.exe`, preparado para suportar a API C#/WinRT nativa no futuro.
- Agendamento em Grafo Direcionado Acíclico (DAG) com execução paralela em Tokio.
- Detecção inteligente de drift de configuração via hashes SHA-256 em labels OCI.
- Interpolação completa de variáveis de ambiente e `.env` (padrão Compose Spec).
- Extensões específicas de WSL via arquivo complementar `rcompose.yml` (estilo Rancher Compose: GPUs, sessão, limites).

---

## 2. Estrutura do Workspace Rust

O projeto é estruturado como um Cargo Workspace multi-crate para garantir modularidade estrita, desacoplamento e testabilidade:

```
rcompose/
├── Cargo.toml                     # Cargo Workspace raiz
├── crates/
│   ├── rcompose-spec/             # Parser, interpolação, modelos de dados e rcompose.yml
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── model.rs           # Project, Service, PortMapping, VolumeMount, etc.
│   │       ├── parser.rs          # serde_yaml + carregador de compose files
│   │       ├── interpolation.rs   # ${VAR:-default}, ${VAR?err}, .env resolver
│   │       ├── extension.rs       # rcompose.yml overlay parser
│   │       └── validation.rs      # Validações semânticas e schema checks
│   ├── rcompose-engine/           # Abstração de container e driver do WSLC
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── engine.rs          # Trait ContainerEngine
│   │       ├── wslc/
│   │       │   ├── mod.rs         # WslcEngine implementação do trait
│   │       │   ├── discovery.rs   # Localização de wslc.exe no host/WSL
│   │       │   ├── command.rs     # Montagem dos argumentos wslc (run, build, etc.)
│   │       │   ├── retry.rs       # Resiliência contra locks transitórios
│   │       │   ├── paths.rs       # Tradução de caminhos Windows <-> Linux
│   │       │   └── types.rs       # Structs serde para JSON do wslc inspect/list
│   │       └── mock.rs            # MockEngine para testes unitários do orquestrador
│   ├── rcompose-core/             # Orquestrador, DAG scheduler e reconciliação
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── dag.rs             # Grafo com petgraph, topological sort e ciclos
│   │       ├── scheduler.rs       # Execução paralela de nós com Tokio tasks
│   │       ├── drift.rs           # Gerador de hash SHA-256 e detector de mudanças
│   │       └── orchestrator.rs    # Operações up, down, restart, build, reconcile
│   └── rcompose-cli/              # Binário rcompose.exe
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── cli.rs             # Definição de comandos Clap v4
│           ├── commands/          # Implementação de up, down, ps, logs, exec, etc.
│           ├── ui.rs              # Spinners, barras de progresso (indicatif)
│           ├── logs.rs            # Multiplexador de logs coloridos com prefixos
│           └── signals.rs         # Captura de Ctrl+C e graceful shutdown
└── tests/                         # Testes de integração end-to-end
```

---

## 3. Modelo de Especificação e Extensões (`rcompose-spec`)

### 3.1 Descoberta e Prioridade de Arquivos
1. **Compose Base**:
   - `compose.yaml`
   - `compose.yml`
   - `docker-compose.yaml`
   - `docker-compose.yml`
   - Flag explícita `-f` / `--file <caminho>`
2. **Extensão Opcional `rcompose.yml`**:
   - Se presente no mesmo diretório ou via `--rcompose-file`, faz overlay/merge sobre o compose base.
3. **Ambiente e `.env`**:
   - Arquivo `.env` na raiz do projeto (ou via `--env-file`).
   - Resolução de variáveis de ambiente com precedência: Flags de linha de comando > Process Environment > `.env` > Defaults `${VAR:-default}`.

### 3.2 Suporte a Sintaxe de Interpolação
- `${VAR}`: substitui pelo valor de `VAR`.
- `${VAR:-default}`: substitui por `default` se `VAR` não estiver definida ou for vazia.
- `${VAR-default}`: substitui por `default` se `VAR` não estiver definida.
- `${VAR:?error_msg}`: encerra a execução com erro caso `VAR` não esteja definida ou for vazia.
- `$$`: caractere literal `$`.

### 3.3 Extensões WSL no `rcompose.yml`
```yaml
version: "3.8"
services:
  app:
    wsl:
      session: "my-custom-session"  # Parâmetro --session do wslc
      gpus: "all"                  # Suporte a GPU CUDA/DirectML
      memory_mb: 4096              # Limite de memória RAM
      cpus: 2                      # Alocação de vCPUs
    restart_policy:
      max_retries: 5
```

---

## 4. Camada de Engine e Driver do WSLC (`rcompose-engine`)

### 4.1 Trait `ContainerEngine`
```rust
#[async_trait]
pub trait ContainerEngine: Send + Sync {
    async fn ping(&self) -> Result<()>;
    async fn list_containers(&self, filter_label: &str) -> Result<Vec<ContainerSummary>>;
    async fn inspect_container(&self, id_or_name: &str) -> Result<ContainerDetails>;
    async fn run_container(&self, opts: RunOptions) -> Result<String>;
    async fn start_container(&self, id_or_name: &str) -> Result<()>;
    async fn stop_container(&self, id_or_name: &str, timeout_secs: u32) -> Result<()>;
    async fn remove_container(&self, id_or_name: &str, force: bool) -> Result<()>;
    async fn create_network(&self, name: &str) -> Result<()>;
    async fn list_networks(&self) -> Result<Vec<String>>;
    async fn create_volume(&self, name: &str) -> Result<()>;
    async fn list_volumes(&self) -> Result<Vec<String>>;
    async fn build_image(&self, opts: BuildOptions) -> Result<()>;
    async fn image_exists(&self, image: &str) -> Result<bool>;
}
```

### 4.2 Resiliência a Locks do Windows e WSLC Preview
O preview do WSL Containers possui condições de corrida conhecidas ao liberar recursos do kernel após interrupções de containers:
- `ERROR_SHARING_VIOLATION` (código `0x80070020` / string Windows)
- `ERROR_ALREADY_EXISTS` (código `0x800700B7` / objeto ainda registrado)

O `rcompose-engine` intercepta essas exceções e executa um loop com backoff exponencial:
- Tentativas: até 5 vezes.
- Intervalo inicial: 1.0s, incrementando para 2.0s.
- Feedback transparente para o usuário informando que o sistema está aguardando liberação de lock.

### 4.3 Tradução de Caminhos e Montagens
- Converte caminhos relativos para absolutos do host Windows (`C:\...`).
- Suporta mapeamento de bind mounts (`host_path:container_path[:ro]`).
- Se executado sob WSL, converte paths `/mnt/<drive>/...` ou via `wslpath -w`.

---

## 5. Orquestração e Agendamento em Grafo (`rcompose-core`)

### 5.1 Grafo DAG e Detecção de Ciclos
- Constrói um grafo direcionado usando o crate `petgraph::graph::DiGraph`.
- Nós: Serviços do Compose.
- Arestas: Relações de dependência extraídas de `depends_on`.
- Executa algoritmo de ordenação topológica com detecção de ciclos (`petgraph::algo::toposort`). Ciclos geram erro explícito detalhando os nós envolvidos.

### 5.2 Execução Paralela com Tokio
- Nós sem dependências mútuas são executados simultaneamente através de `tokio::spawn` e `FuturesUnordered`.
- Um nó dependente aguarda o sinal de conclusão (ou health check) de todas as suas dependências antes de iniciar.

### 5.3 Detecção de Drift e Idempotência
1. Cada serviço tem sua especificação (imagem, portas, volumes, variáveis, comando, limites) convertida em JSON canônico e hasheada com SHA-256 (primeiros 16 caracteres).
2. O hash é salvo na label `com.docker.compose.config-hash`.
3. Ao executar `rcompose up`:
   - Se o container existe, está rodando e o hash bate: **Up-to-date** (nenhuma ação).
   - Se o container existe, o hash bate e está parado: **Start** (não recria).
   - Se o hash mudou ou o usuário passou `--force-recreate`: **Recreate** (para o container antigo, remove e sobe um novo).

---

## 6. Interface de Linha de Comando (`rcompose-cli`)

### 6.1 Subcomandos Principais
- `rcompose up [-d] [--build] [--remove-orphans] [--force-recreate] [SERVICES...]`
- `rcompose down [-v] [--remove-orphans] [-t TIMEOUT]`
- `rcompose ps`
- `rcompose logs [-f] [-t] [-n TAIL] [SERVICES...]`
- `rcompose build [--no-cache] [--pull] [SERVICES...]`
- `rcompose start [SERVICES...]`
- `rcompose stop [SERVICES...]`
- `rcompose restart [SERVICES...]`
- `rcompose exec [-it] SERVICE COMMAND...`
- `rcompose run SERVICE COMMAND...`
- `rcompose config`
- `rcompose version`

### 6.2 UX de Terminal
- Indicadores de progresso e spinners através de `indicatif`.
- Multiplexação de logs coloridos com prefixos alinhados por serviço (`[web-1] | ...`, `[db-1] | ...`).
- Captura de sinal `Ctrl+C` via `tokio::signal::ctrl_c()`, executando parada limpa antes de encerrar o processo.

---

## 7. Estratégia de Testes

1. **Testes Unitários em `rcompose-spec`**:
   - Parsing de múltiplos arquivos Compose reais (Compose Spec v2/v3).
   - Interpolação de variáveis e casos limites (`${VAR:-default}`, `${VAR?error}`).
   - Overlay e validação de `rcompose.yml`.
2. **Testes Unitários em `rcompose-core`**:
   - Resolução de grafos de dependência e detecção de ciclos.
   - Teste de ordenação paralela e reconciliação de drift usando `MockEngine`.
3. **Testes de Integração com `wslc.exe`**:
   - Comandos reais `up`, `ps`, `logs`, `down` executados em ambiente Windows com `wslc.exe` instalado.
