window.PRODUCT_SITE = {
  name: "fox-cull",
  mark: "FC",
  kicker: "Free desktop culling for photos and video",
  headline: "Move through a messy shoot with speed and intent.",
  subhead: "fox-cull gives you a fast first-pass workspace for rating, picking, rejecting, tagging, trimming, and sweeping media without importing or modifying originals.",
  repoUrl: "https://github.com/kumaradarsh1993/fox-cull",
  scene: "culling",
  theme: {
    bg: "#f6f8f4",
    ink: "#152019",
    accent: "#1f8a70",
    accent2: "#d94d3f",
    accent3: "#2469a6"
  },
  downloads: [
    {
      label: "Download for Windows",
      note: "Stable v0.3.0 installer",
      href: "https://github.com/kumaradarsh1993/fox-cull/releases/download/v0.3.0/fox-cull_0.3.0_x64-setup.exe"
    },
    {
      label: "Download for macOS",
      note: "Apple silicon DMG",
      href: "https://github.com/kumaradarsh1993/fox-cull/releases/download/v0.3.0/fox-cull_0.3.0_aarch64.dmg"
    },
    {
      label: "Download for Linux",
      note: "AppImage",
      href: "https://github.com/kumaradarsh1993/fox-cull/releases/download/v0.3.0/fox-cull_0.3.0_amd64.AppImage"
    },
    {
      label: "Beta builds",
      note: "Advanced releases",
      href: "https://github.com/kumaradarsh1993/fox-cull/releases"
    }
  ],
  secondary: [
    { label: "View source", href: "https://github.com/kumaradarsh1993/fox-cull" },
    { label: "All releases", href: "https://github.com/kumaradarsh1993/fox-cull/releases" }
  ],
  stage: {
    title: "Shoot review lane",
    status: "Stable download ready",
    rail: [["Rate", "Stars"], ["Pick", "Keepers"], ["Sweep", "Rejects"]],
    surfaceTitle: "Media grid",
    tiles: ["raw", "jpeg", "clip", "burst", "portrait", "reject", "keeper", "tag", "trim"],
    note: "Designed for the first decision pass, before heavy editors and catalog tools get involved."
  },
  storyTitle: "Fast decisions, tidy folders",
  storyIntro: "The app stays close to the filesystem. You keep control of your media while the interface removes friction from the first pass.",
  chapters: [
    {
      title: "Load a folder",
      body: "Point fox-cull at a shoot and it builds a quick workspace from the files already on disk."
    },
    {
      title: "Decide at speed",
      body: "Use ratings, picks, rejects, tags, focus view, and video trim points to separate keepers from noise."
    },
    {
      title: "Sweep the rejects",
      body: "Move or organize rejected files only when you are ready, while originals remain untouched during review."
    }
  ],
  downloadTitle: "Stable first, beta when curious",
  downloadIntro: "Most users should start with the stable installer. Beta releases are where newer workflow experiments appear first.",
  panels: [
    {
      title: "Stable",
      body: "Recommended for real shoots and day-to-day media review."
    },
    {
      title: "Beta",
      body: "Good for testing newer culling and media workflow changes before they become stable."
    },
    {
      title: "Open source",
      body: "The project is public, so downloads, issues, and release history are easy to inspect."
    }
  ],
  setupTitle: "How to start",
  setupIntro: "Install the app, choose a folder, and begin with the stable review flow.",
  setup: [
    { title: "Install fox-cull", body: "Use the stable download for your operating system." },
    { title: "Open your shoot folder", body: "No import ceremony. The app works from your existing files." },
    { title: "Mark and sweep", body: "Rate, pick, reject, tag, trim, then move rejects when you choose." }
  ],
  footer: "Open-source first-pass media culling for people who need speed without subscription baggage."
};
