export default function Home() {
  const pipeline = [
    {
      label: "Ingest",
      title: "Capture the raw material of real work",
      copy:
        "Sessions, documents, repos, notes, and exports become durable memory instead of disposable context.",
    },
    {
      label: "Consolidate",
      title: "Compress noise into usable structure",
      copy:
        "Memento chunks, links, scores, and learns from what matters so later queries return signal, not transcript sludge.",
    },
    {
      label: "Retrieve",
      title: "Bring back the right memory at the right depth",
      copy:
        "Keyword search, semantic retrieval, provenance, and local-first storage make recall practical for humans and agents.",
    },
  ];

  const signals = [
    "Local-first engine",
    "Daemon + CLI loop",
    "Optional sync later",
    "Semantic + lexical retrieval",
  ];

  return (
    <main className="relative overflow-hidden px-6 py-8 md:px-10 lg:px-14">
      <div className="mx-auto flex min-h-screen w-full max-w-7xl flex-col gap-8">
        <header className="fade-up flex flex-col gap-5 rounded-[2rem] border border-[var(--line)] bg-[rgba(255,251,245,0.58)] px-5 py-5 backdrop-blur md:flex-row md:items-center md:justify-between md:px-7">
          <div>
            <p className="font-display text-3xl tracking-[0.14em] text-[var(--foreground)] uppercase">
              Memento
            </p>
            <p className="mt-1 max-w-xl text-sm text-[var(--muted)] md:text-base">
              Intelligent memory for agents that need continuity, not just context windows.
            </p>
          </div>
          <div className="flex flex-wrap gap-2 text-xs uppercase tracking-[0.16em] text-[var(--muted)]">
            {signals.map((signal) => (
              <span
                key={signal}
                className="rounded-full border border-[var(--line)] bg-[var(--panel-strong)] px-3 py-2"
              >
                {signal}
              </span>
            ))}
          </div>
        </header>

        <section className="grid gap-6 lg:grid-cols-[1.25fr_0.75fr]">
          <article className="glass-panel fade-up-delay relative rounded-[2.4rem] px-6 py-8 md:px-10 md:py-12">
            <div className="signal-line absolute inset-x-8 top-6 hidden md:block" />
            <p className="mb-4 inline-flex rounded-full border border-[var(--line)] bg-[var(--panel-strong)] px-4 py-2 text-xs font-medium uppercase tracking-[0.2em] text-[var(--muted)]">
              Local-first memory engine
            </p>
            <h1 className="font-display max-w-4xl text-5xl leading-[0.9] tracking-tight text-[var(--foreground)] md:text-7xl">
              Remember the work.
              <span className="block text-[var(--accent)]">Not just the prompt.</span>
            </h1>
            <p className="mt-6 max-w-2xl text-base leading-8 text-[var(--muted)] md:text-xl">
              Memento turns conversations, documents, code, and decisions into a durable semantic memory layer.
              The engine stays close to the machine, the daemon keeps it available, and the interface brings back
              the exact context you actually need.
            </p>

            <div className="mt-8 grid gap-3 sm:grid-cols-3">
              <div className="ink-card rounded-[1.6rem] p-4">
                <p className="text-xs uppercase tracking-[0.18em] text-[var(--muted)]">Core</p>
                <p className="mt-3 font-display text-3xl">Rust engine</p>
                <p className="mt-2 text-sm leading-6 text-[var(--muted)]">
                  Chunking, consolidation, eigenspace learning, and `.memento` storage.
                </p>
              </div>
              <div className="ink-card rounded-[1.6rem] p-4">
                <p className="text-xs uppercase tracking-[0.18em] text-[var(--muted)]">Interface</p>
                <p className="mt-3 font-display text-3xl">Daemon + CLI</p>
                <p className="mt-2 text-sm leading-6 text-[var(--muted)]">
                  A practical loop for import, learn, query, and status without cloud lock-in.
                </p>
              </div>
              <div className="ink-card rounded-[1.6rem] p-4">
                <p className="text-xs uppercase tracking-[0.18em] text-[var(--muted)]">Surface</p>
                <p className="mt-3 font-display text-3xl">Memory UX</p>
                <p className="mt-2 text-sm leading-6 text-[var(--muted)]">
                  A web layer that explains, explores, and eventually talks to your memory graph.
                </p>
              </div>
            </div>
          </article>

          <aside className="fade-up-delay-2 flex flex-col gap-4">
            <div className="glass-panel rounded-[2rem] p-5">
              <p className="text-xs uppercase tracking-[0.18em] text-[var(--muted)]">
                Working loop
              </p>
              <div className="mt-4 space-y-3 font-mono text-sm leading-7 text-[var(--foreground)]">
                <div className="rounded-2xl border border-[var(--line)] bg-[rgba(255,250,244,0.9)] p-4">
                  <span className="text-[var(--accent)]">$</span> memento import claude ./session.jsonl
                </div>
                <div className="rounded-2xl border border-[var(--line)] bg-[rgba(255,250,244,0.9)] p-4">
                  <span className="text-[var(--accent)]">$</span> memento learn
                </div>
                <div className="rounded-2xl border border-[var(--line)] bg-[rgba(255,250,244,0.9)] p-4">
                  <span className="text-[var(--accent)]">$</span> memento query &quot;what did we decide about auth?&quot;
                </div>
              </div>
            </div>

            <div className="rounded-[2rem] bg-[#17110e] px-5 py-6 text-[#f5ecdf] shadow-[0_24px_60px_rgba(22,16,12,0.28)]">
              <p className="text-xs uppercase tracking-[0.18em] text-[#d4b79b]">Why this matters</p>
              <p className="mt-4 font-display text-4xl leading-none">
                Context windows forget.
              </p>
              <p className="mt-3 text-sm leading-7 text-[#dccdbc]">
                Teams lose rationale. Agents lose continuity. Memento stores the trail of work in a form that can
                be queried later with precision.
              </p>
            </div>
          </aside>
        </section>

        <section className="grid gap-4 lg:grid-cols-3">
          {pipeline.map((item) => (
            <article key={item.label} className="ink-card rounded-[2rem] p-6">
              <p className="text-xs uppercase tracking-[0.18em] text-[var(--accent)]">{item.label}</p>
              <h2 className="mt-4 font-display text-4xl leading-tight text-[var(--foreground)]">
                {item.title}
              </h2>
              <p className="mt-4 text-sm leading-7 text-[var(--muted)]">{item.copy}</p>
            </article>
          ))}
        </section>

        <section className="glass-panel rounded-[2.2rem] px-6 py-7 md:px-8">
          <div className="grid gap-8 md:grid-cols-[0.9fr_1.1fr] md:items-end">
            <div>
              <p className="text-xs uppercase tracking-[0.18em] text-[var(--muted)]">Current frame</p>
              <h2 className="mt-3 font-display text-5xl leading-none text-[var(--foreground)]">
                One repo. One memory engine. Many surfaces.
              </h2>
            </div>
            <div className="grid gap-3 text-sm leading-7 text-[var(--muted)] md:grid-cols-2">
              <div className="rounded-[1.4rem] border border-[var(--line)] bg-[var(--panel-strong)] p-4">
                <p className="font-mono text-xs uppercase tracking-[0.18em] text-[var(--accent)]">libmemento</p>
                Engine, format, retrieval, learning.
              </div>
              <div className="rounded-[1.4rem] border border-[var(--line)] bg-[var(--panel-strong)] p-4">
                <p className="font-mono text-xs uppercase tracking-[0.18em] text-[var(--accent)]">mementod</p>
                Local service and memory management.
              </div>
              <div className="rounded-[1.4rem] border border-[var(--line)] bg-[var(--panel-strong)] p-4">
                <p className="font-mono text-xs uppercase tracking-[0.18em] text-[var(--accent)]">memento-cli</p>
                Import, query, status, learn.
              </div>
              <div className="rounded-[1.4rem] border border-[var(--line)] bg-[var(--panel-strong)] p-4">
                <p className="font-mono text-xs uppercase tracking-[0.18em] text-[var(--accent)]">memento-web</p>
                Product communication and future memory UX.
              </div>
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}
