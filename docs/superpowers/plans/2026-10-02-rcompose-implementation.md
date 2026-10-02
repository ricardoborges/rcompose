# rcompose Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Construir o `rcompose`, um orquestrador estilo Docker Compose em Rust para containers nativos do Windows WSL (`wslc.exe`).

**Architecture:** Cargo Workspace modular de 4 crates: `rcompose-spec` (parsing de Compose YAML, interpolação de variáveis e overlay de `rcompose.yml`), `rcompose-engine` (trait `ContainerEngine`, driver assíncrono para `wslc.exe`, retries de locks do Windows), `rcompose-core` (agendador DAG com `petgraph`, concorrência Tokio e detector de drift SHA-256), e `rcompose-cli` (interface Clap v4, UI com `indicatif`, logs multiplexados coloridos e graceful shutdown).

**Tech Stack:** Rust 1.92+, Cargo Workspace, `tokio` (async/process), `clap v4`, `serde`/`serde_yaml`/`serde_json`, `petgraph`, `indicatif`, `colored`, `which`, `dotenvy`, `regex`.

**Spec:** [docs/superpowers/specs/2026-10-02-rcompose-design.md](file:///d:/dev/github/ricardoborges/rcompose/docs/superpowers/specs/2026-10-02-rcompose-design.md)

## Global Constraints

- **OS / Shell**: Windows 11, PowerShell terminal (`Get-Command`, `cargo`, etc.).
- **Plataforma alvo**: `x86_64-pc-windows-msvc` (binário nativo `rcompose.exe`).
- **Compatibilidade**: WSL Container preview (`C:\Program Files\WSL\wslc.exe` ou `wslc.exe` no PATH).
- **Tratamento de locks**: Retries automáticos para erros transitórios `ERROR_SHARING_VIOLATION` (0x80070020) e `ERROR_ALREADY_EXISTS` (0x800700B7).
- **Sem stubs / sem placeholders**: Todas as etapas devem conter código funcional e testes correspondentes.

---

### Task 1: Setup do Cargo Workspace

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `crates/rcompose-spec/Cargo.toml`
- Create: `crates/rcompose-spec/src/lib.rs`
- Create: `crates/rcompose-engine/Cargo.toml`
- Create: `crates/rcompose-engine/src/lib.rs`
- Create: `crates/rcompose-core/Cargo.toml`
- Create: `crates/rcompose-core/src/lib.rs`
- Create: `crates/rcompose-cli/Cargo.toml`
- Create: `crates/rcompose-cli/src/main.rs`

**Interfaces:**
- Produces: Estrutura inicial do workspace compilável via `cargo check --workspace`.

- [ ] **Step 1: Criar o `.gitignore` e `Cargo.toml` raiz do workspace**

```toml
[workspace]
resolver = "2"
members = [
    "crates/rcompose-spec",
    "crates/rcompose-engine",
    "crates/rcompose-core",
    "crates/rcompose-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Ricardo Borges"]
license = "MIT"
```

- [ ] **Step 2: Criar os `Cargo.toml` e stubs iniciais dos 4 crates**

Configurar dependências compartilhadas e referências locais entre crates:
- `rcompose-spec`: `serde`, `serde_yaml`, `serde_json`, `regex`, `dotenvy`, `thiserror`.
- `rcompose-engine`: `rcompose-spec`, `async-trait`, `tokio`, `which`, `thiserror`, `serde`, `serde_json`.
- `rcompose-core`: `rcompose-spec`, `rcompose-engine`, `petgraph`, `tokio`, `sha2`, `thiserror`.
- `rcompose-cli`: `rcompose-spec`, `rcompose-engine`, `rcompose-core`, `clap` (derive), `tokio` (rt-multi-thread, signal), `indicatif`, `colored`, `anyhow`.

- [ ] **Step 3: Verificar compilação do workspace**

Run: `cargo check --workspace`
Expected: Conclusão com sucesso sem erros.

- [ ] **Step 4: Commit**

```powershell
git add .gitignore Cargo.toml crates/
git commit -m "chore: initialize cargo workspace with 4 crates"
```

---

### Task 2: Modelo de Dados do Compose (`rcompose-spec`)

**Files:**
- Create: `crates/rcompose-spec/src/model.rs`
- Modify: `crates/rcompose-spec/src/lib.rs`
- Create: `crates/rcompose-spec/tests/model_test.rs`

**Interfaces:**
- Produces: `Project`, `Service`, `PortMapping`, `VolumeMount`, `BuildConfig`, `NetworkConfig`, `VolumeConfig`.

- [ ] **Step 1: Escrever teste de desserialização do modelo**

```rust
// crates/rcompose-spec/tests/model_test.rs
use rcompose_spec::model::*;

#[test]
fn test_port_mapping_parsing() {
    let p1 = PortMapping::parse("8080:80").unwrap();
    assert_eq!(p1.target, 80);
    assert_eq!(p1.published.as_deref(), Some("8080"));
    assert_eq!(p1.protocol, "tcp");

    let p2 = PortMapping::parse("127.0.0.1:53:53/udp").unwrap();
    assert_eq!(p2.target, 53);
    assert_eq!(p2.published.as_deref(), Some("127.0.0.1:53"));
    assert_eq!(p2.protocol, "udp");
}
```

- [ ] **Step 2: Executar teste e verificar falha**

Run: `cargo test -p rcompose-spec --test model_test`
Expected: FAIL (tipos e métodos não definidos).

- [ ] **Step 3: Implementar `model.rs`**

Implementar structs fortemente tipadas com `serde::Deserialize` e `serde::Serialize`:
- `PortMapping` (parsing de strings curtas `"host:container"` e formato estendido).
- `VolumeMount` (tipo `bind`, `volume`, `tmpfs`, flags `read_only`).
- `Service` com todos os campos canônicos do Compose (imagem, build, environment, ports, volumes, networks, depends_on, user, working_dir, etc.).
- `Project` com coleções de serviços, redes e volumes.

- [ ] **Step 4: Executar testes e verificar sucesso**

Run: `cargo test -p rcompose-spec --test model_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-spec/
git commit -m "feat(spec): implement compose data models and port/volume parsers"
```

---

### Task 3: Interpolação de Variáveis e Suporte a `.env` (`rcompose-spec`)

**Files:**
- Create: `crates/rcompose-spec/src/interpolation.rs`
- Modify: `crates/rcompose-spec/src/lib.rs`
- Create: `crates/rcompose-spec/tests/interpolation_test.rs`

**Interfaces:**
- Produces: `interpolate_string(input: &str, env: &HashMap<String, String>) -> Result<String, InterpolationError>`
- Produces: `load_env_file(path: &Path) -> Result<HashMap<String, String>, SpecError>`

- [ ] **Step 1: Escrever testes unitários para regras de interpolação**

Testar casos:
- `${VAR}`
- `${VAR:-default}` (substitui se vazia ou não setada)
- `${VAR-default}` (substitui apenas se não setada)
- `${VAR:?error}` (retorna erro explicativo se não setada)
- `$$` (escapa para `$`)

- [ ] **Step 2: Executar teste e verificar falha**

Run: `cargo test -p rcompose-spec --test interpolation_test`
Expected: FAIL.

- [ ] **Step 3: Implementar `interpolation.rs` com regex/lexer seguro**

Implementar parser de expressões `${...}` com resolução baseada em mapa de variáveis e tratamento de erros amigável. Integrar leitura de arquivos `.env` com fallback para variáveis do sistema (`std::env::vars()`).

- [ ] **Step 4: Executar testes e verificar sucesso**

Run: `cargo test -p rcompose-spec --test interpolation_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-spec/
git commit -m "feat(spec): implement variable interpolation and .env resolution"
```

---

### Task 4: Carregador de Compose e Extensões `rcompose.yml` (`rcompose-spec`)

**Files:**
- Create: `crates/rcompose-spec/src/extension.rs`
- Create: `crates/rcompose-spec/src/loader.rs`
- Modify: `crates/rcompose-spec/src/lib.rs`
- Create: `crates/rcompose-spec/tests/loader_test.rs`

**Interfaces:**
- Produces: `find_compose_file(dir: &Path) -> Option<PathBuf>`
- Produces: `find_rcompose_extension_file(dir: &Path) -> Option<PathBuf>`
- Produces: `load_project(compose_path: &Path, opts: LoadOptions) -> Result<Project, SpecError>`

- [ ] **Step 1: Escrever testes para busca hierárquica e fusão de `rcompose.yml`**

Criar fixtures temporárias em memória ou diretório de testes contendo `compose.yaml` e `rcompose.yml` com extensões WSL (`gpus: all`, `session: dev`), testando o merge no `Project`.

- [ ] **Step 2: Executar teste e verificar falha**

Run: `cargo test -p rcompose-spec --test loader_test`
Expected: FAIL.

- [ ] **Step 3: Implementar `extension.rs` e `loader.rs`**

- Busca automática de arquivos (`compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml`).
- Overlay hierárquico: se `rcompose.yaml` existir, mesclar os campos de `wsl` e substituições de serviço no modelo carregado.
- Resolução e injeção do nome do projeto padrão (baseado no nome do diretório).

- [ ] **Step 4: Executar testes e verificar sucesso**

Run: `cargo test -p rcompose-spec --test loader_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-spec/
git commit -m "feat(spec): implement compose loader and rcompose.yml overlay"
```

---

### Task 5: Abstração `ContainerEngine` e `MockEngine` (`rcompose-engine`)

**Files:**
- Create: `crates/rcompose-engine/src/engine.rs`
- Create: `crates/rcompose-engine/src/mock.rs`
- Modify: `crates/rcompose-engine/src/lib.rs`
- Create: `crates/rcompose-engine/tests/engine_trait_test.rs`

**Interfaces:**
- Produces: `trait ContainerEngine: Send + Sync`
- Produces: `MockEngine` para testes unitários com gravação de chamadas.

- [ ] **Step 1: Escrever teste de comportamento com `MockEngine`**

- [ ] **Step 2: Executar teste e verificar falha**

Run: `cargo test -p rcompose-engine --test engine_trait_test`
Expected: FAIL.

- [ ] **Step 3: Implementar o trait `ContainerEngine` e `MockEngine`**

Definir métodos assíncronos:
- `ping`, `list_containers`, `inspect_container`
- `run_container`, `start_container`, `stop_container`, `remove_container`
- `create_network`, `list_networks`, `create_volume`, `list_volumes`
- `build_image`, `image_exists`

- [ ] **Step 4: Executar testes e verificar sucesso**

Run: `cargo test -p rcompose-engine --test engine_trait_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-engine/
git commit -m "feat(engine): define ContainerEngine trait and MockEngine"
```

---

### Task 6: Driver `WslcEngine` com Retries e Tradução de Caminhos (`rcompose-engine`)

**Files:**
- Create: `crates/rcompose-engine/src/wslc/discovery.rs`
- Create: `crates/rcompose-engine/src/wslc/paths.rs`
- Create: `crates/rcompose-engine/src/wslc/command.rs`
- Create: `crates/rcompose-engine/src/wslc/retry.rs`
- Create: `crates/rcompose-engine/src/wslc/mod.rs`
- Modify: `crates/rcompose-engine/src/lib.rs`
- Create: `crates/rcompose-engine/tests/wslc_command_test.rs`

**Interfaces:**
- Produces: `struct WslcEngine` implementando `ContainerEngine`.
- Produces: `find_wslc_bin() -> Result<PathBuf, EngineError>`

- [ ] **Step 1: Escrever teste de montagem de flags para `wslc run` e `wslc build`**

Verificar se todas as flags OCI, portas, volumes, redes e labels de projeto/serviço/hash são montadas corretamente.

- [ ] **Step 2: Executar teste e verificar falha**

Run: `cargo test -p rcompose-engine --test wslc_command_test`
Expected: FAIL.

- [ ] **Step 3: Implementar módulos de comando, descoberta, retries e caminhos**

- Localização inteligente de `wslc.exe`.
- Conversão de bind mounts para paths absolutos válidos no Windows.
- Parser de JSON para saídas de `wslc list --format json` e `wslc inspect`.
- Loop com backoff de retries para `ERROR_SHARING_VIOLATION` e `ERROR_ALREADY_EXISTS`.

- [ ] **Step 4: Executar testes e verificar sucesso**

Run: `cargo test -p rcompose-engine --test wslc_command_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-engine/
git commit -m "feat(engine): implement WslcEngine driver with retries and path translation"
```

---

### Task 7: Grafo DAG de Dependências e Detecção de Ciclos (`rcompose-core`)

**Files:**
- Create: `crates/rcompose-core/src/dag.rs`
- Modify: `crates/rcompose-core/src/lib.rs`
- Create: `crates/rcompose-core/tests/dag_test.rs`

**Interfaces:**
- Produces: `struct DependencyGraph`
- Produces: `DependencyGraph::from_project(project: &Project) -> Result<DependencyGraph, DagError>`
- Produces: `graph.execution_batches() -> Vec<Vec<String>>` (camadas ordenadas para execução paralela)

- [ ] **Step 1: Escrever testes para ordenação topológica e detecção de ciclos**

Testar:
- Dependência linear: `web` depende de `api`, `api` depende de `db` -> camadas `[[db], [api], [web]]`.
- Execução paralela: `db` e `redis` independentes, `api` depende de ambos -> camadas `[[db, redis], [api]]`.
- Ciclo: `A -> B -> A` -> deve retornar erro `DagError::CircularDependency("A -> B -> A")`.

- [ ] **Step 2: Executar teste e verificar falha**

Run: `cargo test -p rcompose-core --test dag_test`
Expected: FAIL.

- [ ] **Step 3: Implementar `dag.rs` usando `petgraph`**

Montar o grafo direcionado, executar `toposort` e agrupar nós em camadas de concorrência garantindo que dependentes só executam após a camada anterior estar completa.

- [ ] **Step 4: Executar testes e verificar sucesso**

Run: `cargo test -p rcompose-core --test dag_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-core/
git commit -m "feat(core): implement dependency DAG, cycle detection and concurrency batches"
```

---

### Task 8: Gerador de Hash de Configuração e Detecção de Drift (`rcompose-core`)

**Files:**
- Create: `crates/rcompose-core/src/drift.rs`
- Modify: `crates/rcompose-core/src/lib.rs`
- Create: `crates/rcompose-core/tests/drift_test.rs`

**Interfaces:**
- Produces: `compute_config_hash(service: &Service) -> String`
- Produces: `reconcile_service_state(service: &Service, existing: Option<&ContainerDetails>) -> DesiredAction` (UpToDate, Start, Recreate)

- [ ] **Step 1: Escrever testes para hashing e cálculo de drift**

Verificar que:
- Alterar imagem, porta ou variável de ambiente muda o hash.
- Se o container existente tiver o mesmo hash e estiver ativo, a ação é `UpToDate`.
- Se o container tiver hash diferente, a ação é `Recreate`.

- [ ] **Step 2: Executar teste e verificar falha**

Run: `cargo test -p rcompose-core --test drift_test`
Expected: FAIL.

- [ ] **Step 3: Implementar `drift.rs`**

Serialização canônica ordenada da configuração do serviço e geração de hash SHA-256 (16 caracteres hex). Lógica de comparação com labels de containers inspecionados.

- [ ] **Step 4: Executar testes e verificar sucesso**

Run: `cargo test -p rcompose-core --test drift_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-core/
git commit -m "feat(core): implement config hashing and drift reconciler"
```

---

### Task 9: Orquestrador do Ciclo de Vida (`rcompose-core`)

**Files:**
- Create: `crates/rcompose-core/src/orchestrator.rs`
- Modify: `crates/rcompose-core/src/lib.rs`
- Create: `crates/rcompose-core/tests/orchestrator_test.rs`

**Interfaces:**
- Produces: `struct Orchestrator<E: ContainerEngine>`
- Produces: `orchestrator.up(opts: UpOptions) -> Result<()>`
- Produces: `orchestrator.down(opts: DownOptions) -> Result<()>`
- Produces: `orchestrator.ps() -> Result<Vec<ServiceStatus>>`

- [ ] **Step 1: Escrever testes unitários do orquestrador usando `MockEngine`**

Verificar que:
- `up` cria redes e volumes antes de containers.
- Executa nós da mesma camada concorrentemente.
- `down` para os containers na ordem inversa e depois remove redes.

- [ ] **Step 2: Executar teste e verificar falha**

Run: `cargo test -p rcompose-core --test orchestrator_test`
Expected: FAIL.

- [ ] **Step 3: Implementar `orchestrator.rs` com execução assíncrona Tokio**

Implementar fluxos de `up`, `down`, `ps`, `restart`, garantindo tratamento de erros e idempotência.

- [ ] **Step 4: Executar testes e verificar sucesso**

Run: `cargo test -p rcompose-core --test orchestrator_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-core/
git commit -m "feat(core): implement lifecycle orchestrator for up, down, ps"
```

---

### Task 10: CLI e Subcomandos (`rcompose-cli`)

**Files:**
- Create: `crates/rcompose-cli/src/cli.rs`
- Create: `crates/rcompose-cli/src/commands/up.rs`
- Create: `crates/rcompose-cli/src/commands/down.rs`
- Create: `crates/rcompose-cli/src/commands/ps.rs`
- Create: `crates/rcompose-cli/src/commands/logs.rs`
- Create: `crates/rcompose-cli/src/commands/exec.rs`
- Create: `crates/rcompose-cli/src/commands/config.rs`
- Create: `crates/rcompose-cli/src/commands/mod.rs`
- Modify: `crates/rcompose-cli/src/main.rs`

**Interfaces:**
- Produces: Executável `rcompose.exe` com todos os comandos da especificação.

- [ ] **Step 1: Configurar estrutura Clap v4 com subcomandos e flags**

- [ ] **Step 2: Implementar handlers de cada comando conectando a `rcompose-spec`, `rcompose-engine` e `rcompose-core`**

- [ ] **Step 3: Testar execução de `--help` e comandos via cargo**

Run: `cargo run -p rcompose-cli -- --help`
Expected: Exibição completa de opções de ajuda dos comandos.

- [ ] **Step 4: Commit**

```powershell
git add crates/rcompose-cli/
git commit -m "feat(cli): wire up clap commands to engine and core orchestrator"
```

---

### Task 11: Experiência de Terminal, Logs Multiplexados e Signals (`rcompose-cli`)

**Files:**
- Create: `crates/rcompose-cli/src/ui.rs`
- Create: `crates/rcompose-cli/src/logs.rs`
- Create: `crates/rcompose-cli/src/signals.rs`
- Modify: `crates/rcompose-cli/src/main.rs`

**Interfaces:**
- Produces: Spinners e barras de progresso via `indicatif`.
- Produces: Log multiplexer com cores ANSI dedicadas por serviço.
- Produces: Graceful shutdown em `tokio::signal::ctrl_c()`.

- [ ] **Step 1: Implementar spinners do `indicatif` para criação de redes, volumes e containers**

- [ ] **Step 2: Implementar multiplexador assíncrono de logs (`logs.rs`) com cores ANSI e prefixos alinhados**

- [ ] **Step 3: Implementar interceptação de Ctrl+C no `main.rs`**

- [ ] **Step 4: Testar visualmente a compilação do binário**

Run: `cargo build --workspace`
Expected: Compilação limpa de todos os crates e binário `target/debug/rcompose.exe`.

- [ ] **Step 5: Commit**

```powershell
git add crates/rcompose-cli/
git commit -m "feat(cli): add indicatif progress UI, colored log multiplexer and graceful shutdown"
```

---

### Task 12: Teste de Integração de Ponta a Ponta com WSLC

**Files:**
- Create: `examples/basic-web/compose.yaml`
- Create: `examples/basic-web/rcompose.yaml`
- Create: `tests/integration_wslc.rs`

**Interfaces:**
- Produces: Teste de validação real executando `rcompose.exe up -d`, `rcompose.exe ps` e `rcompose.exe down`.

- [ ] **Step 1: Criar exemplo simples `examples/basic-web/compose.yaml`**

```yaml
version: "3.8"
services:
  web:
    image: nginx:alpine
    ports:
      - "8888:80"
```

- [ ] **Step 2: Executar validação de configuração via `rcompose config`**

Run: `cargo run -p rcompose-cli -- -f examples/basic-web/compose.yaml config`
Expected: Emissão da configuração resolvida em YAML.

- [ ] **Step 3: Executar teste de integração com `wslc.exe`**

Testar ciclo completo no Windows com `wslc.exe` disponível.

- [ ] **Step 4: Commit**

```powershell
git add examples/ tests/
git commit -m "test: add integration test and basic-web example"
```
