import * as THREE from "https://cdn.jsdelivr.net/npm/three@0.170.0/build/three.module.js";

const data = window.PRODUCT_SITE;
const root = document.querySelector("#site-root");
const product = data.product || data.scene;
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

document.documentElement.style.setProperty("--accent", data.theme.accent);
document.documentElement.style.setProperty("--accent-2", data.theme.accent2);
document.documentElement.style.setProperty("--accent-3", data.theme.accent3);
document.documentElement.style.setProperty("--bg", data.theme.bg);
document.documentElement.style.setProperty("--ink", data.theme.ink);
document.documentElement.dataset.product = product;

root.innerHTML = `
  <header class="nav">
    <div class="nav-inner">
      <a class="brand" href="#top"><span class="mark">${data.mark}</span><span>${data.name}</span></a>
      <nav class="nav-links" aria-label="Primary">
        <a href="#story">How it works</a>
        <a href="#download">Download</a>
        <a href="#setup">Setup</a>
        <a class="nav-pill" href="${data.repoUrl}">GitHub</a>
      </nav>
    </div>
    <div class="progress"><span id="page-progress"></span></div>
  </header>

  <main id="top">
    <section class="section hero">
      <div class="hero-copy reveal">
        <span class="eyebrow">${data.kicker}</span>
        <h1>${data.headline}</h1>
        <p class="lede">${data.subhead}</p>
        <div class="download-grid">
          ${data.downloads.map((item, index) => `
            <a class="download-card ${index === 0 ? "primary" : ""}" href="${item.href}">
              <span>${item.label}</span><small>${item.note}</small>
            </a>
          `).join("")}
        </div>
        <div class="secondary-actions">
          ${data.secondary.map((item) => `<a class="button" href="${item.href}">${item.label}</a>`).join("")}
        </div>
      </div>

      <div class="hero-visual reveal" data-product-stage="${product}">
        <div class="world"><canvas id="world-canvas" aria-hidden="true"></canvas></div>
        ${renderHeroStage()}
      </div>
    </section>

    <section class="section proof-strip reveal" aria-label="Highlights">
      ${data.proof.map((item) => `<span>${item}</span>`).join("")}
    </section>

    <section class="section story" id="story">
      <div class="story-head reveal">
        <span class="section-kicker">Product script</span>
        <h2>${data.storyTitle}</h2>
        <p>${data.storyIntro}</p>
      </div>

      <div class="story-flow">
        <aside class="story-screen reveal" aria-live="polite">
          <div class="screen-top">
            <span id="flow-step">01</span>
            <strong id="flow-tag">${data.beats[0].tag}</strong>
          </div>
          <div class="flow-visual" id="flow-visual">${renderBeatVisual(0)}</div>
          <div class="flow-copy">
            <h3 id="flow-title">${data.beats[0].title}</h3>
            <p id="flow-body">${data.beats[0].body}</p>
          </div>
          <div class="flow-meter"><span id="flow-meter"></span></div>
        </aside>

        <div class="beats">
          ${data.beats.map((beat, index) => `
            <article class="beat ${index === 0 ? "active" : ""}" data-beat="${index}">
              <div class="beat-mobile-visual">${renderBeatVisual(index)}</div>
              <span>${String(index + 1).padStart(2, "0")} / ${beat.tag}</span>
              <h3>${beat.title}</h3>
              <p>${beat.body}</p>
            </article>
          `).join("")}
        </div>
      </div>
    </section>

    <section class="section download-section" id="download">
      <div class="section-head reveal">
        <span class="section-kicker">Download</span>
        <h2>${data.downloadTitle}</h2>
        <p>${data.downloadIntro}</p>
      </div>
      <div class="panel-grid">
        ${data.panels.map((panel) => `
          <article class="info-panel reveal">
            <h3>${panel.title}</h3>
            <p>${panel.body}</p>
          </article>
        `).join("")}
      </div>
    </section>

    <section class="section setup-section" id="setup">
      <div class="section-head reveal">
        <span class="section-kicker">Setup</span>
        <h2>${data.setupTitle}</h2>
        <p>${data.setupIntro}</p>
      </div>
      <div class="setup-steps">
        ${data.setup.map((step) => `
          <div class="setup-step reveal">
            <strong>${step.title}</strong>
            <span>${step.body}</span>
          </div>
        `).join("")}
      </div>
    </section>
  </main>

  <footer class="footer">
    <span>${data.name} by Kumar Adarsh</span>
    <span>${data.footer}</span>
  </footer>
`;

let sceneTarget = 0;
let refs = {};
const pointer = { x: 0, y: 0 };

setupScrollDemo();
setupScrollSpy();
setupAnimation();
try {
  setupWorld();
} catch (error) {
  console.warn("WebGL scene skipped", error);
  document.querySelector(".world")?.remove();
}

function renderHeroStage() {
  if (product === "dictation") return renderDictationHero();
  if (product === "markdown") return renderMarkdownHero();
  return renderCullingHero();
}

function renderDictationHero() {
  const hero = data.hero;
  return `
    <div class="stage dictation-stage">
      <div class="mini-apps">
        ${hero.apps.map((app) => `<span>${app}</span>`).join("")}
      </div>
      <div class="dictation-console">
        <div class="console-top">
          <strong>${hero.title}</strong>
          <span>${hero.status}</span>
        </div>
        <div class="avatar-orbit">
          <img src="${data.assets.avatar}" alt="" />
          <div class="speech-bubble">Listening...</div>
        </div>
        <div class="wave">${Array.from({ length: 20 }, (_, i) => `<span style="--i:${i}"></span>`).join("")}</div>
        <div class="mode-row">
          ${hero.modes.map((mode) => `<div><kbd>${mode[0]}</kbd><span>${mode[1]}</span></div>`).join("")}
        </div>
        <div class="transcript-card">
          ${hero.transcript.map((line) => `<p>${line}</p>`).join("")}
        </div>
        <div class="paste-card">${hero.output}</div>
      </div>
    </div>
  `;
}

function renderMarkdownHero() {
  const hero = data.hero;
  return `
    <div class="stage reader-stage">
      <div class="reader-window">
        <div class="reader-tabs">
          ${hero.tabs.map((tab, index) => `<span class="${index === 0 ? "active" : ""}">${tab}</span>`).join("")}
        </div>
        <div class="reader-body">
          <aside class="reader-files">
            ${hero.files.map((file, index) => `<span class="${index === 0 ? "active" : ""}">${file}</span>`).join("")}
          </aside>
          <article class="reader-doc">
            <div class="doc-toolbar">
              ${hero.widthStates.map((state, index) => `<span class="${index === 1 ? "active" : ""}">${state}</span>`).join("")}
            </div>
            <h3>${hero.headings[0]}</h3>
            ${hero.paragraphs.map((paragraph, index) => `<p class="${index === 2 ? "changed" : ""}">${paragraph}</p>`).join("")}
            <div class="doc-code">Ctrl + ] widens the page. Ctrl + \\ uses the full window.</div>
          </article>
          <aside class="diff-rail">
            <strong>Agent edits</strong>
            <span>3 changed sections</span>
            <span>Beta theatre</span>
          </aside>
        </div>
      </div>
    </div>
  `;
}

function renderCullingHero() {
  const hero = data.hero;
  return `
    <div class="stage cull-stage">
      <div class="cull-window">
        <aside class="folder-tree">
          <strong>${hero.title}</strong>
          ${hero.folders.map((folder, index) => `<span class="${index === 0 ? "active" : ""}">${folder}</span>`).join("")}
        </aside>
        <div class="media-workspace">
          <div class="media-toolbar">
            ${hero.chips.map((chip, index) => `<span class="${index === 0 ? "active" : ""}">${chip}</span>`).join("")}
          </div>
          <div class="media-grid">
            ${hero.media.map((item, index) => `<div class="media-tile ${index === 8 ? "reject" : index === 7 ? "pick" : ""}"><span>${item}</span></div>`).join("")}
          </div>
          <div class="key-strip">
            ${hero.keys.map((key) => `<div><kbd>${key[0]}</kbd><span>${key[1]}</span></div>`).join("")}
          </div>
        </div>
      </div>
    </div>
  `;
}

function renderBeatVisual(index) {
  const beat = data.beats[index] || data.beats[0];
  if (product === "dictation") return renderDictationBeat(beat, index);
  if (product === "markdown") return renderMarkdownBeat(beat, index);
  return renderCullingBeat(beat, index);
}

function renderDictationBeat(beat, index) {
  if (beat.visual === "modes") {
    return `
      <div class="beat-dictation modes-demo">
        ${data.hero.modes.map((mode, i) => `<div class="${i === index % 3 ? "active" : ""}"><kbd>${mode[0]}</kbd><strong>${mode[1]}</strong><span>${i === 0 ? "Fast" : i === 1 ? "Shaped" : "Readable"}</span></div>`).join("")}
      </div>
    `;
  }
  if (beat.visual === "paste") {
    return `
      <div class="paste-demo">
        <div class="app-card active">Gmail<span>Your cursor is here</span></div>
        <div class="app-card">Slack<span>Team update</span></div>
        <div class="app-card">Docs<span>Meeting notes</span></div>
        <div class="paste-line">wispr-fox pastes the cleaned text into the active field.</div>
      </div>
    `;
  }
  if (beat.visual === "avatar") {
    return `
      <div class="avatar-demo">
        <img src="${data.assets.avatar}" alt="" />
        <div class="avatar-list">
          <span class="active">Fox</span><span>Paperclip</span><span>Real Clippy</span><span>Desk Cat</span>
        </div>
      </div>
    `;
  }
  return `
    <div class="listen-demo">
      <kbd>F8</kbd>
      <div class="large-wave">${Array.from({ length: 28 }, (_, i) => `<span style="--i:${i}"></span>`).join("")}</div>
      <p>Recording from the app you were already using</p>
    </div>
  `;
}

function renderMarkdownBeat(beat) {
  if (beat.visual === "width") {
    return `
      <div class="width-demo">
        <div class="width-track"><span></span></div>
        <div class="page-widths"><span>Narrow</span><span class="active">Comfort</span><span>Full window</span></div>
        <article><h4>The document should fit the screen.</h4><p>The reading column expands instead of wasting both sides of your monitor.</p></article>
      </div>
    `;
  }
  if (beat.visual === "tabs") {
    return `
      <div class="tabs-demo">
        <div class="tab-row"><span>brief.md</span><span class="active">plan.md</span><span>notes.md</span><span class="tear">Torn out</span></div>
        <div class="files-outline"><span>Files</span><span>Outline</span><span>Release notes</span></div>
      </div>
    `;
  }
  if (beat.visual === "diff") {
    return `
      <div class="diff-demo">
        <article><h4>Agent edit detected</h4><p class="fresh">Fresh green highlight while the text is changing.</p><p class="stale">Yellow highlight after the edit settles.</p></article>
        <aside><strong>Changed sections</strong><span>Summary</span><span>Naive diff</span></aside>
      </div>
    `;
  }
  return `
    <div class="open-md-demo">
      <div class="file-stack"><span>.md</span><span>.md</span><span>.md</span></div>
      <article><h4>Plain Markdown in, readable page out</h4><p>No vault, no import, no project setup.</p></article>
    </div>
  `;
}

function renderCullingBeat(beat) {
  if (beat.visual === "keys") {
    return `
      <div class="keys-demo">
        ${data.hero.keys.map((key) => `<div><kbd>${key[0]}</kbd><strong>${key[1]}</strong></div>`).join("")}
        <div class="label-row"><span class="blue"></span><span class="red"></span><span class="green"></span><span class="yellow"></span></div>
      </div>
    `;
  }
  if (beat.visual === "video") {
    return `
      <div class="video-demo">
        <div class="video-frame">4K60 HEVC poster</div>
        <div class="scrub"><span></span></div>
        <div class="trim-row"><span>In</span><strong>Lossless trim</strong><span>Out</span></div>
      </div>
    `;
  }
  if (beat.visual === "trash") {
    return `
      <div class="trash-demo">
        <div class="trash-grid"><span>Reject</span><span>Reject</span><span>Restore</span></div>
        <div class="trash-actions"><button>Restore</button><button>Delete forever</button></div>
        <p>Recoverable by default until you empty it.</p>
      </div>
    `;
  }
  return `
    <div class="folder-demo">
      <aside><span>E:/Photos</span><span class="active">Phone 2026</span><span>DJI</span><span>Archive</span></aside>
      <div class="folder-grid">${Array.from({ length: 8 }, (_, i) => `<span>${i % 3 === 0 ? "RAW" : i % 3 === 1 ? "JPG" : "MOV"}</span>`).join("")}</div>
    </div>
  `;
}

function setupAnimation() {
  const progress = document.querySelector("#page-progress");
  const gsap = window.gsap;
  if (!gsap || !window.ScrollTrigger || reduceMotion) {
    if (progress) progress.style.width = "100%";
    return;
  }
  gsap.registerPlugin(window.ScrollTrigger);
  gsap.to(progress, {
    width: "100%",
    ease: "none",
    scrollTrigger: { trigger: document.body, start: "top top", end: "bottom bottom", scrub: 0.35 }
  });
  gsap.utils.toArray(".reveal").forEach((el) => {
    gsap.fromTo(el, { y: 34, opacity: 0 }, {
      y: 0,
      opacity: 1,
      duration: 0.65,
      ease: "power3.out",
      scrollTrigger: { trigger: el, start: "top 88%", once: true }
    });
  });
}

function setupScrollDemo() {
  refs = {
    step: document.querySelector("#flow-step"),
    tag: document.querySelector("#flow-tag"),
    visual: document.querySelector("#flow-visual"),
    title: document.querySelector("#flow-title"),
    body: document.querySelector("#flow-body"),
    meter: document.querySelector("#flow-meter"),
    beats: [...document.querySelectorAll("[data-beat]")]
  };
  setSceneTarget(0);
}

function setupScrollSpy() {
  const beats = [...document.querySelectorAll("[data-beat]")];
  if (!beats.length) return;

  let ticking = false;
  const update = () => {
    ticking = false;
    const center = innerHeight * 0.56;
    let closest = 0;
    let distance = Infinity;
    beats.forEach((beat, index) => {
      const rect = beat.getBoundingClientRect();
      const d = Math.abs(rect.top + rect.height * 0.5 - center);
      if (d < distance) {
        closest = index;
        distance = d;
      }
    });
    setSceneTarget(closest);
  };
  const queue = () => {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(update);
  };
  addEventListener("scroll", queue, { passive: true });
  addEventListener("resize", queue, { passive: true });
  queue();
}

function setSceneTarget(value) {
  const index = Math.max(0, Math.min(data.beats.length - 1, value));
  sceneTarget = index;
  const beat = data.beats[index];
  if (!beat || !refs.visual) return;

  refs.step.textContent = String(index + 1).padStart(2, "0");
  refs.tag.textContent = beat.tag;
  refs.title.textContent = beat.title;
  refs.body.textContent = beat.body;
  refs.visual.innerHTML = renderBeatVisual(index);
  refs.meter.style.width = `${((index + 1) / data.beats.length) * 100}%`;
  refs.beats.forEach((el, beatIndex) => el.classList.toggle("active", beatIndex === index));
}

window.addEventListener("pointermove", (event) => {
  pointer.x = (event.clientX / Math.max(1, innerWidth) - 0.5) * 2;
  pointer.y = (event.clientY / Math.max(1, innerHeight) - 0.5) * 2;
}, { passive: true });

function setupWorld() {
  const canvas = document.querySelector("#world-canvas");
  if (!canvas) return;
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(Math.min(devicePixelRatio || 1, 2));

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(38, 1, 0.1, 100);
  camera.position.set(0, 0.25, 7.2);

  scene.add(new THREE.AmbientLight(0xffffff, 1.1));
  const key = new THREE.DirectionalLight(0xffffff, 2.5);
  key.position.set(3, 4, 5);
  scene.add(key);
  const wash = new THREE.PointLight(data.theme.accent, 54, 10);
  wash.position.set(-3, -1.5, 4.5);
  scene.add(wash);

  const group = new THREE.Group();
  scene.add(group);
  buildWorld(group);
  addParticleField(group);

  function resize() {
    const width = Math.max(canvas.clientWidth, 1);
    const height = Math.max(canvas.clientHeight, 1);
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  }

  function tick(now) {
    resize();
    const t = now * 0.001;
    const target = sceneTarget * 0.35;
    group.rotation.y += ((target + pointer.x * 0.14) - group.rotation.y) * 0.035;
    group.rotation.x += ((Math.sin(t * 0.4) * 0.06 - pointer.y * 0.09) - group.rotation.x) * 0.04;
    group.position.y = Math.sin(t * 0.35) * 0.05;
    group.position.x = innerWidth < 720 ? 0.25 : 0.55;
    group.scale.setScalar(innerWidth < 720 ? 0.56 : innerWidth < 980 ? 0.74 : 0.92);
    group.children.forEach((child, index) => {
      if (child.userData.baseY !== undefined && !reduceMotion) {
        child.position.y = child.userData.baseY + Math.sin(t * child.userData.float + index) * 0.026;
      }
    });
    renderer.render(scene, camera);
    requestAnimationFrame(tick);
  }
  requestAnimationFrame(tick);
}

function buildWorld(group) {
  if (product === "dictation") return buildDictationWorld(group);
  if (product === "markdown") return buildMarkdownWorld(group);
  return buildCullWorld(group);
}

function buildDictationWorld(group) {
  const mic = new THREE.Mesh(new THREE.CapsuleGeometry(0.32, 1.15, 16, 32), mat(data.theme.accent));
  mic.userData = { baseY: 0, float: 1.1 };
  group.add(mic);
  for (let i = 0; i < 3; i++) {
    const ring = new THREE.Mesh(new THREE.TorusGeometry(0.84 + i * 0.42, 0.012, 12, 96), mat(i % 2 ? data.theme.accent2 : data.theme.accent));
    ring.rotation.x = Math.PI / 2;
    ring.userData = { baseY: 0, float: 0.7 + i * 0.2 };
    group.add(ring);
  }
  for (let i = 0; i < 14; i++) addBox(group, -1.8 + i * 0.28, -1.35, -0.25, 0.07, 0.2 + (i % 5) * 0.13, 0.07, i % 2 ? data.theme.accent2 : data.theme.accent3);
}

function buildMarkdownWorld(group) {
  addBox(group, 0, 0, 0, 2.2, 3.1, 0.08, "#ffffff");
  addBox(group, -1.28, -0.05, -0.14, 0.82, 2.75, 0.08, "#20283a");
  for (let i = 0; i < 8; i++) {
    const wide = i % 3 === 0 ? 0.72 : 1.05;
    addBox(group, 0.1, 1.12 - i * 0.3, 0.14, wide, 0.06, 0.05, i === 5 ? data.theme.accent3 : data.theme.accent);
  }
  addBox(group, 1.15, -0.65, 0.22, 0.5, 1.25, 0.06, data.theme.accent2);
}

function buildCullWorld(group) {
  for (let i = 0; i < 10; i++) {
    const x = -1.55 + (i % 5) * 0.78;
    const y = 0.95 - Math.floor(i / 5) * 0.72;
    const color = i % 3 === 0 ? data.theme.accent : i % 3 === 1 ? data.theme.accent2 : data.theme.accent3;
    const tile = addBox(group, x, y, i * 0.025, 0.62, 0.44, 0.07, color);
    tile.rotation.z = (i - 5) * 0.015;
  }
  for (let i = 0; i < 7; i++) addBox(group, -1.7 + i * 0.55, -1.25, 0.24, 0.38, 0.2, 0.06, i % 2 ? data.theme.accent2 : data.theme.accent);
}

function addParticleField(group) {
  const geometry = new THREE.BufferGeometry();
  const positions = [];
  for (let i = 0; i < 90; i++) positions.push((Math.random() - 0.5) * 7, (Math.random() - 0.5) * 4.6, (Math.random() - 0.5) * 2.8 - 0.6);
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  const field = new THREE.Points(geometry, new THREE.PointsMaterial({ color: data.theme.accent, size: 0.018, transparent: true, opacity: 0.24 }));
  field.userData = { baseY: field.position.y, float: 0.4 };
  group.add(field);
}

function addBox(group, x, y, z, w, h, d, color) {
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), mat(color));
  mesh.position.set(x, y, z);
  mesh.userData = { baseY: y, float: 0.55 + Math.random() * 0.8 };
  group.add(mesh);
  return mesh;
}

function mat(color) {
  return new THREE.MeshStandardMaterial({
    color,
    roughness: 0.54,
    metalness: 0.06,
    transparent: true,
    opacity: 0.86
  });
}
