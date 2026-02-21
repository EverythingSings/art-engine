# EverythingSings.art — Architecture v0.4

> A generative art engine (codename TBD) with a CLI interface, a two-agent system that operates it, and a gallery that stores what gets made. Deployed as a Railway template. Decisions locked: one vision-capable model, two agents (Operator + Critic), WebGL rendering, stream-everything UI, impressively expressive V1, engine gets its own name.

---

## 01 — The Idea in One Breath

You deploy a Railway template. It gives you a generative art engine (Rust → WASM → WebGL), a CLI to control it, and a two-agent system (via OpenRouter) that can drive the same CLI. An Operator agent translates your intent into CLI commands; a Critic agent looks at the rendered output and pushes for improvements. Everything streams to the frontend — you watch the agents think, argue, and create in real-time. Humans can drive the engine directly or collaborate with the agents. The engine is the product. The agents are just very capable users.

The engine has its own name and identity, separate from EverythingSings.art. EverythingSings is the brand; the engine is the tool.

---

## 02 — Conceptual Components

### The Engine (Core)
Generative art engine in Rust, compiled to WASM for browser and native for server. Renders via WebGL for GPU-accelerated visuals including shaders, particle systems, and post-processing. Exposes a CLI command interface. Impressively expressive from V1 — not a toy demo but a real creative tool with deep primitives. The engine has its own name (TBD), separate from the EverythingSings brand.

**Tech:** Rust, WASM, WebGL

### The CLI (Interface)
Command interface to the engine. Works as a real CLI and as a web terminal. The canonical way anything — human or agent — talks to the engine. Includes `--help`, state introspection, and structured output modes for agent consumption.

**Tech:** Rust, CLI/REPL

### The Agent System (Intelligence)
A two-agent orchestration layer. An Operator translates intent into CLI commands; a Critic evaluates rendered output via vision. Both use the same model (one vision-capable model via OpenRouter). Everything the agents do streams to the frontend in real-time — full transparency. **Stretch goal, but designed in from the start.**

**Tech:** OpenRouter, Two-Agent, Rust

### The Gallery (Persistence)
Database-backed collection. Stores command histories (replayable), rendered snapshots, agent conversations, metadata. Each deployment is its own curated art space.

**Tech:** Postgres, Rust, Web UI

### The Frontend (Presentation)
Split-pane interface. Live WASM/WebGL canvas on one side, terminal/chat hybrid on the other. Gallery below. The canvas is the live output of the engine. **Everything streams:** agent reasoning, CLI commands being issued, engine responses, critic evaluations — all visible in real-time. Transparent by design. Watch the machine think.

**Tech:** HTML/JS, WASM bindings, WebSocket

### The Backend (Infrastructure)
Rust HTTP server on Railway. Serves frontend, runs agent orchestration, proxies OpenRouter, manages gallery, and hosts a headless engine instance for server-side rendering.

**Tech:** Rust/Axum, Postgres, Railway

---

## 03 — The Feedback Loop (Stretch Goal)

This is the most important and hardest piece of the agent system. The agents must see the output of their commands to iterate meaningfully. There are three distinct channels of feedback, each serving a different purpose.

### Channel 1 — CLI Output (Text)
Every CLI command returns structured text: confirmations, error messages, current state summaries. Free, instant, always available. The Operator agent works primarily on this channel — fast, cheap, text-only iteration.

### Channel 2 — Screenshot (Vision)
A rendered frame captured as a PNG and sent to the vision model. This is how the Critic "sees" the art. Expensive (vision tokens cost more), slower (render + encode + API call), but irreplaceable for aesthetic judgment. Only used at review checkpoints, not every command.

### Channel 3 — Metrics (Structured Data)
Computed properties of the current render: color histogram, particle density distribution, average velocity, frame rate, spatial entropy, symmetry score. Cheap to compute, useful for the Operator to reason about composition without burning vision tokens. The engine exports these via a `stats` CLI command.

### Cost Optimization
The Operator works on Channels 1+3 (fast, cheap). The Critic uses Channel 2 only at decision points — after a batch of changes, not after every single command. With one model (Gemini Flash), a full piece with 5 iterations might cost $0.01-0.05.

---

## 04 — The Two-Agent System (Stretch Goal)

~~Three agents~~ → **Two agents, one model.** The Planner and Operator roles are collapsed into a single Operator agent that both interprets user intent and writes CLI commands. The Critic remains separate — it's the one that sees screenshots. Both agents use the same vision-capable model via OpenRouter. Simpler config, fewer moving parts, same core loop.

### Agent 1: The Operator
Interprets user intent, reasons about artistic direction, AND translates it into concrete CLI commands. This agent does the creative thinking and the technical execution in one context. It knows the full CLI reference, sees text/metrics feedback, and iterates on its own commands. Does not see screenshots — that's the Critic's job.

- **Receives:** User's natural language request. CLI reference documentation. Current engine state. CLI text responses. Stats/metrics JSON. Critic's feedback from prior iterations.
- **Produces:** Sequences of CLI commands. Self-iterates via text feedback. Signals "ready for review" when a pass is complete.
- **Model:** Vision-capable (same model as Critic, but text-only calls here — no images sent).

### Agent 2: The Critic
The only agent that sees the rendered output. Receives a screenshot + the original user intent + what the Operator was trying to achieve. Evaluates: does this achieve the vision? Produces structured feedback or approves the piece. The aesthetic judge.

- **Receives:** Screenshot (vision). User's original intent. Current metrics. Iteration count.
- **Produces:** Verdict: APPROVE or REFINE. If REFINE: structured critique (what's working, what's not, specific suggestions). Fed back to Operator for next iteration.
- **Model:** Vision-capable (same model, but this is the call that includes the image).

### Why Two Agents, Not One or Three
One agent that plans, executes, AND critiques its own work has no external perspective — it's grading its own homework. Three agents (Planner, Operator, Critic) adds orchestration complexity and inter-agent communication overhead for marginal benefit. Two agents is the sweet spot: the Operator creates, the Critic evaluates. Clean separation of concerns. Generator → Critic pattern.

### Why One Model
Using one vision-capable model for both agents simplifies everything: one env var (`OPENROUTER_MODEL`), one API config, predictable costs. The Operator just doesn't get image inputs. The Critic does. Same model, different context windows. Users can swap the model freely — Claude Sonnet, GPT-4o, Gemini Flash, whatever works on OpenRouter with vision.

### Two-Agent Loop — Full Cycle

```
User: "make something that feels like bioluminescence underwater"

→ Operator receives intent + CLI docs + engine state
  Reasons about artistic direction internally:
    "Deep ocean palette, scattered light points, organic motion..."
  Executes CLI commands:
    canvas new 1024 1024 --bg #020210
    palette create biolum --colors #020210,#0a2a4a,#00ccaa,#ffffff
    layer add deep --type particles --count 1500
    particles configure deep --palette biolum --glow 2.0 --size 1..6
    field add drift --type curl-noise --scale 0.003 --strength 0.5
    layer add pulse --type particles --count 50
    particles configure pulse --color #00ffcc --glow 4.0 --lifetime 30..80
    post add bloom --radius 12 --intensity 0.6
    post add grain --amount 0.015
    render start
  Self-checks via text responses + stats
  Signals "ready for review"

→ Screenshot captured (client-side canvas.toDataURL)

→ Critic receives screenshot + original intent + metrics
  "Strong base. Dark background and glow work well. But motion is
   too uniform — bioluminescence has clusters and voids. Need more
   spatial variation. Pulse layer should be more sporadic. Push
   color deeper into teal."
  Verdict: REFINE

→ Operator receives critique + engine state
  Adjusts:
    field add attractor --type point --x 0.3 --y 0.6 --strength 0.4
    field add attractor --type point --x 0.7 --y 0.2 --strength 0.3
    particles configure pulse --emission sporadic --interval 40..120
    palette adjust biolum --shift-hue -15
  Signals "ready for review"

→ Screenshot → Critic → APPROVE

→ gallery save --name "deep glow" --tags bioluminescence,particles
```

**Everything above streams to the frontend in real-time.** The user watches the Operator's reasoning, sees commands execute, watches the canvas update, reads the Critic's evaluation. Full transparency.

### Orchestration
A Rust orchestrator on the backend manages the loop. Each agent is a separate OpenRouter API call with its own system prompt. The orchestrator:

1. Receives user message via WebSocket
2. Calls Operator (text-only) → gets CLI commands
3. Executes commands, streams to frontend
4. Operator self-refinement: 1-3 text-only sub-iterations
5. Triggers screenshot capture from frontend via WebSocket
6. Calls Critic (vision) → gets APPROVE or REFINE
7. If REFINE and iteration < max: go to step 2 with feedback
8. If APPROVE: save to gallery

Budgets: max 5 outer iterations, 3 operator sub-iterations each. Hard limits on total token cap and wall-clock timeout.

### Failure Modes
- **Infinite refinement:** Critic never approves. Hard cap at N iterations (configurable, default 5). Return best attempt with quality note.
- **Operator hallucinating commands:** CLI parser rejects invalid commands and returns error text. Operator gets error in next context, can self-correct. After 3 consecutive errors, abort that sub-iteration.
- **Token budget exhaustion:** Track cumulative tokens per session. Warn at 80% budget. Hard stop at 100%. Save current state so user can continue manually.

---

## 05 — OpenRouter Model Landscape (Stretch Goal Reference)

One model for everything. Must be vision-capable (for the Critic). The Operator uses the same model but without image inputs.

### Vision-Capable Models on OpenRouter

| Model | Fit | Cost (in/out per M) | Notes |
|-------|-----|------|-------|
| Gemini 2.5 Flash | ★ Recommended default | $0.15/$0.60 | Cheap, fast, good vision + code. Best cost/quality ratio for iterative loops. |
| Claude Sonnet 4.5 | ★ Premium | $3/$15 | Best aesthetic reasoning and code generation. Expensive for iteration. |
| GPT-4o | ★ Strong | $2.50/$10 | Reliable vision and structured output. Good middle ground. |
| Qwen3-VL-235B | ◆ Viable | $0.20/$0.88 | Strong multimodal. Budget-friendly alternative. |
| Qwen 2.5-VL-72B (free) | ◆ Demo tier | Free | Quality varies. Works for testing/demos. |
| Llama 3.2 11B Vision (free) | ✕ Weak | Free | Vision exists but aesthetic reasoning limited. Not recommended. |

### Recommendation
Default to **Gemini 2.5 Flash**. It handles both roles (Operator text reasoning + Critic vision) at $0.15/M input. A full piece with 5 iterations might cost $0.01-0.05. Users can swap to Claude Sonnet or GPT-4o for higher quality via a single env var.

---

## 06 — Screenshot Capture

### Option A: Client-Side Capture (V1)
Browser renders WASM canvas → `canvas.toDataURL()` → WebSocket to backend → backend forwards to OpenRouter as base64 image. User sees exactly what the Critic sees.

**Pros:** Simple. Uses real rendering context.
**Cons:** Requires browser to be open. Agent can't work autonomously.

### Option B: Server-Side Headless (V2+)
Backend runs engine natively (same Rust, no browser) → renders to offscreen buffer → encodes PNG → sends to OpenRouter. Enables autonomous agent sessions and batch generation.

**Pros:** Autonomous. Batch generation. Same codebase.
**Cons:** Headless rendering context adds Docker complexity.

**V1 Recommendation:** Client-side only. Add server-side headless in V2.

---

## 07 — Orchestration — How Agents Communicate (V2)

The agents don't talk to each other directly. A Rust orchestrator on the backend manages the loop.

### Sequence
1. Receive user message via WebSocket
2. Call **Planner** via OpenRouter (text-only)
3. Call **Operator** via OpenRouter (text-only) → receives CLI commands
4. Execute commands against engine, collect text responses, stream to frontend
5. *(Optional)* Operator self-refinement: 1-3 text-only sub-iterations
6. Trigger snapshot via WebSocket → frontend captures canvas → sends PNG back
7. Call **Critic** via OpenRouter (vision) → receives APPROVE or REFINE
8. If REFINE and iteration < max: go to step 2. If APPROVE: save to gallery.

---

## 08 — The Engine — Domain Primitives

The engine should be impressively expressive from V1. Not a tech demo with one particle effect — a real creative tool that rewards exploration. The CLI surface should feel like a deep instrument.

### Core Primitives

| Primitive | Description |
|-----------|-------------|
| **Canvas** | Dimensions, background color/gradient, clear, global blend mode. The root context. |
| **Layers** | Stackable rendering layers with independent blend modes (add, multiply, screen, overlay, etc.) and opacity. Composited in order. |
| **Particles** | Point systems: position, velocity, acceleration, lifetime, emission rate/pattern (burst, continuous, sporadic), size range, color (from palette or explicit), glow/bloom intensity, trails (length, fade). The workhorse. |
| **Fields** | Vector/scalar fields that influence particles and shapes. Perlin noise, simplex noise, curl noise, Worley/cellular noise, attractors (point, line, orbital), repulsors, gravity wells, turbulence, vortex. Fields are composable — stack multiple fields on a layer. |
| **Shapes** | Geometric primitives: circles, lines, polygons, bezier curves, spirals, grids, rings. Can be static, animated, or used as particle emitters. Stroke and fill with palette colors. |
| **Shaders** | Custom fragment shaders for per-pixel effects on layers. Ship with a library of built-in shaders (wave distortion, voronoi, fractal, reaction-diffusion, kaleidoscope, liquid, fire, electric). Users/agents can combine shaders. This is where WebGL earns its keep. |
| **Palettes** | Named color palettes with generation rules: analogous, complementary, triadic, split-complementary, from-image, gradient ramp, custom. Palettes are first-class — everything references them. Built-in curated palettes (deep ocean, neon, earth, monochrome, vapor, etc.). |
| **Transforms** | Rotation, scale, translation, shear, mirror. Applied to layers, shapes, or particle systems. Animatable with easing curves (linear, ease-in-out, elastic, bounce, cubic-bezier). |
| **Time / Animation** | Global frame counter, speed multiplier, pause/resume, seek to frame. Per-element animation: oscillation, phase offset, frequency. Easing functions library. The engine is inherently temporal — art moves. |
| **Composition** | Layer ordering, masking (alpha mask one layer with another), clipping regions, tiling/repetition, symmetry modes (radial, bilateral, kaleidoscopic). |
| **Post-processing** | Bloom, gaussian blur, chromatic aberration, film grain, vignette, color grading (lift/gamma/gain), dithering, pixelation, edge detection, feedback/echo (previous frame blended into current). Stackable — order matters. |
| **Stats** | Introspection: color histogram, dominant colors, spatial density map, particle velocity distribution, entropy score, symmetry score, luminance distribution. Structured JSON. Cheap to compute. |
| **Snapshot / Export** | Capture current frame as PNG (configurable resolution). Export full command history as replayable session file. Save to gallery with metadata and tags. |

### Built-in Shader Library (V1)

| Shader | Description |
|--------|-------------|
| `wave` | Sinusoidal displacement of pixels. Frequency, amplitude, direction. |
| `voronoi` | Voronoi cell pattern. Scale, edge thickness, cell coloring from palette. |
| `fractal` | Mandelbrot/Julia set rendering. Zoom, center, iteration depth, palette mapping. |
| `reaction-diffusion` | Gray-Scott model. Feed/kill rates. Produces organic, coral-like patterns. |
| `kaleidoscope` | Radial symmetry reflection. Segment count, rotation speed. |
| `liquid` | Fluid simulation approximation. Viscosity, surface tension. |
| `displacement` | Distort layer using another layer or noise field as displacement map. |
| `feedback` | Mix previous frame into current. Decay rate, offset. Creates trails and echoes. |

### Why This Level of Expressiveness
The engine needs to be worth deploying *without* the agent. If someone opens the CLI and starts typing commands, they should be able to make genuinely beautiful things within minutes. The shader library and deep particle/field system make this possible. The agent benefits too — a richer command surface gives it more creative vocabulary.

---

## 09 — System Architecture

```
┌─── Railway Deployment ─────────────────────────────────────────────┐
│                                                                    │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │  Backend Service (Rust / Axum)                              │  │
│  │                                                             │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐    │  │
│  │  │ HTTP Server   │  │ Orchestrator │  │ Engine (CLI)   │    │  │
│  │  │ + WebSocket   │  │ (V2)         │  │                │    │  │
│  │  │ static files  │  │ Planner call │  │ Processes CLI  │    │  │
│  │  │ API routes    │  │ Operator call│  │ commands       │    │  │
│  │  │ gallery API   │  │ Critic call  │  │ Returns text   │    │  │
│  │  │              │  │ loop control │  │ Returns stats  │    │  │
│  │  └──────────────┘  └──────────────┘  └────────────────┘    │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                    │
│  ┌──────────────────────────────────────┐                        │
│  │  PostgreSQL                          │                        │
│  │  gallery, sessions, conversations    │                        │
│  └──────────────────────────────────────┘                        │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
         │                              │
         ▼                              ▼
  OpenRouter API               Browser (User)
  (V2 - agent models)         WASM Engine + Canvas
                               Terminal / Chat UI
                               Gallery view
                               WebSocket ↔ backend
```

---

## 10 — Railway Template Configuration

**Template Name:** [Engine Name TBD] — by EverythingSings

> One-click deploy: a generative art engine with WebGL rendering, expressive CLI, personal gallery, and optional AI agent mode. Create WASM generative art through direct CLI control or AI-assisted conversation. Everything streams live.

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ADMIN_PASSWORD` | Password for studio access (required) |
| `OPENROUTER_API_KEY` | Your OpenRouter key (enables agent mode, optional) |
| `OPENROUTER_MODEL` | Vision-capable model (default: google/gemini-2.5-flash) |
| `MAX_ITERATIONS` | Max agent loop iterations (default: 5) |
| `GALLERY_PUBLIC` | Public gallery access (default: true) |
| `DATABASE_URL` | Auto-configured by Railway (Postgres plugin) |

### Railway Services
- **App:** Single Rust binary. Serves frontend, runs engine, hosts agent orchestration. Docker build.
- **PostgreSQL:** Gallery storage, sessions, command histories. Auto-provisioned by template.

---

## 11 — Security Model

Each deployment is a personal instance — one person's art studio. The threat model is simple: protect the studio and your OpenRouter credits from unauthorized access while keeping the gallery publicly viewable.

### Two Zones

**Public Zone (no auth):**
- `GET /gallery` — Browse saved pieces
- `GET /gallery/:id` — View a single piece (live replay)
- `GET /gallery/:id/img` — Thumbnail/snapshot image
- `GET /` — Landing page / gallery home

**Admin Zone (password required):**
- `GET /studio` — Engine + CLI + canvas interface
- `WS /ws` — WebSocket (CLI commands, agent stream)
- `POST /gallery` — Save a new piece
- `DEL /gallery/:id` — Delete a piece
- `PUT /gallery/:id` — Edit piece metadata
- `POST /agent/start` — Start agent loop (burns OpenRouter $)
- `POST /agent/stop` — Stop agent loop

### Implementation

| Aspect | Approach |
|--------|----------|
| **Auth mechanism** | Single admin password set via `ADMIN_PASSWORD` env var in Railway. |
| **Login flow** | Simple login page → password checked server-side → session token issued as HTTP-only cookie. |
| **Session tokens** | HMAC-signed, time-limited (24h default). No raw password in cookie. Stateless verification. |
| **Protected routes** | Axum middleware checks session cookie on all admin-zone routes. 401 → redirect to login. |
| **WebSocket auth** | Session token validated on WS handshake. No anonymous WebSocket connections to studio. |
| **HTTPS** | Railway provides TLS by default on generated domains. No extra config. |
| **Brute force** | Rate-limit login endpoint: 5 attempts per minute per IP. In-memory counter. |

### Why This Is Enough for V1
There is exactly one user per deployment: the owner. No user accounts, no roles, no OAuth. The password protects two things — the ability to create/destroy art and the ability to spend money via OpenRouter. A signed session cookie over HTTPS handles this cleanly. Multi-user (collaborators, guest artists) is a V2+ concern.

---

## 12 — Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Engine is the product** | Agents are optional. Engine + CLI must be compelling alone. Deploy without an OpenRouter key and you still get a manual art studio. |
| **Impressively expressive V1** | Not a toy. Shaders, deep particle systems, composable fields, post-processing stack. The engine should make people say "wait, this is free?" |
| **WebGL rendering** | GPU-accelerated. Enables shaders, high particle counts, real-time post-processing. This is what makes the engine visually powerful. |
| **CLI as API contract** | Agent, frontend, scripts, humans — all use the same commands. Keeps the agent layer thin and replaceable. |
| **Two agents, one model** | Operator (creates) + Critic (evaluates). Generator → Critic pattern. One vision-capable model for both, configured via single env var. |
| **Stream everything** | All agent reasoning, commands, engine responses, and critic evaluations stream to the frontend live. Full transparency. Watch the machine think. |
| **Replayable, not static** | Gallery pieces are command sequences. Replay, fork, remix. Art as code, code as art. |
| **Sovereign by default** | Each deployment is fully self-contained. Your art, your data, your models, your instance. |
| **Engine has its own name** | Separate from EverythingSings brand. The engine is a tool; EverythingSings is the brand that created it. |

---

## 13 — Open Questions

### Resolved ✓

| Question | Decision |
|----------|----------|
| Single model vs multi-model? | **One model.** Single `OPENROUTER_MODEL` env var. Must be vision-capable. |
| How many agents? | **Two.** Operator + Critic. Generator → Critic pattern. |
| Frontend transparency? | **Stream everything.** Full visibility into agent process. |
| Rendering approach? | **WebGL.** GPU-accelerated, enables shaders and high particle counts. |
| How expressive should V1 be? | **Impressively.** Shaders, deep particle systems, composable fields, post-processing. Not a toy. |
| Engine naming? | **Separate name** from EverythingSings. TBD. |

### Still Open

**What's the engine name?**
Needs its own identity. Should evoke generative art, creation, emergence. Short, memorable, domain-available. The Railway template will be listed under this name.

**Shader authoring: built-in only or user-writable?**
V1 ships built-in shaders. But should the CLI allow writing custom GLSL fragments? Huge expressiveness boost but also a security/complexity surface. Could be a V2 feature.

**Gallery piece format: static snapshots or live embeds?**
Stored command histories can be replayed live (visitor sees the piece animate). But that means loading WASM for every gallery view. Alternative: store a rendered GIF/video alongside the command history. Both? Live by default with a static fallback?

**Mobile experience?**
The split-pane terminal + canvas is a desktop-first interface. Is a simplified mobile view worth building for V1, or is this explicitly a desktop tool?

---

## 14 — Build Order

| Phase | Scope |
|-------|-------|
| **Phase 1** | Engine core — canvas, layers, particles, fields (perlin, curl, simplex, attractors), shapes, palettes (built-in + generation), transforms with easing. Compiles to WASM. Renders via WebGL. CLI works. Stats command works. |
| **Phase 2** | Shaders + post-processing — built-in shader library (wave, voronoi, fractal, reaction-diffusion, kaleidoscope, liquid, displacement, feedback). Post-processing stack (bloom, blur, chromatic aberration, grain, vignette, color grading). Composition (masking, symmetry, tiling). |
| **Phase 3** | Backend — Axum server, serves frontend + WASM. Postgres. Gallery save/load/list API. Security (admin password, session auth). Snapshot/export. |
| **Phase 4** | Frontend — split-pane UI: WebGL canvas + terminal + gallery. WebSocket for real-time CLI interaction. Stream-everything design for terminal output. |
| **Phase 5** | Railway template — Dockerfile, env vars, Postgres plugin config, template metadata, marketplace listing. **Start earning kickbacks here.** |
| **Phase 6** *(stretch)* | Two-agent system — Operator + Critic. Single model via OpenRouter. Screenshot capture via WebSocket. Orchestrator loop. Stream agent reasoning to frontend. |
| **Phase 7+** *(future)* | Server-side headless rendering. Custom shader authoring. Audio reactivity. Piece forking/remixing. Community features. Mobile view. |

> **The engine must impress before the agents arrive.** Phases 1-2 are where the product earns its reputation. If the CLI + WebGL rendering can make someone go "holy shit" in 5 minutes of typing commands, everything else follows.
