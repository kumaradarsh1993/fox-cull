window.PRODUCT_SITE = {
  name: "fox-cull",
  mark: "FC",
  product: "culling",
  kicker: "Fast open-source photo and video culling",
  headline: "Clean out a camera roll without building a Lightroom catalog.",
  subhead: "fox-cull is a lightweight first-pass media manager for phone dumps, camera cards, drone clips, and family folders. Open a folder where it already lives, rate, reject, tag, preview, trim, and sweep clutter without importing or touching originals until you choose.",
  insight: "The product is for the neglected middle: people with too many photos and videos, but no desire to maintain a heavyweight catalog just to delete the bad ones.",
  repoUrl: "https://github.com/kumaradarsh1993/fox-cull",
  scene: "culling",
  theme: {
    bg: "#f6f8f4",
    ink: "#152019",
    accent: "#1f8a70",
    accent2: "#d94d3f",
    accent3: "#2469a6"
  },
  assets: {
    logo: "images/fox-cull-icon.png"
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
      note: "Newer media workflow work",
      href: "https://github.com/kumaradarsh1993/fox-cull/releases"
    }
  ],
  secondary: [
    { label: "View source", href: "https://github.com/kumaradarsh1993/fox-cull" },
    { label: "All releases", href: "https://github.com/kumaradarsh1993/fox-cull/releases" }
  ],
  proof: [
    "Browse a native folder in place, including subfolders",
    "Grid, details, focus, and filmstrip views",
    "Stars, color labels, pick/reject, tags, and filters",
    "Video posters, scrubbing, HEVC fallback, and trim export"
  ],
  hero: {
    title: "Native folder library",
    status: "Stable culling flow",
    folders: ["Phone 2026", "Osmo Pocket", "Mavic Mini", "Family archive"],
    chips: ["Grid", "Details", "Focus", "Month groups", "Reject filter"],
    media: ["IMG_1042", "PXL_3811", "DJI_0082", "VID_4K60", "RAW_211", "Portrait", "Burst", "Keep", "Reject"],
    keys: [
      ["X", "Reject"],
      ["P", "Pick"],
      ["1-5", "Rate"],
      ["L", "Lights out"]
    ]
  },
  storyTitle: "A culling workflow, not another archive chore",
  storyIntro: "Open the mess where it lives. Make quick decisions. Delete safely only when you are ready.",
  beats: [
    {
      title: "Open a folder or drive in place",
      body: "Point fox-cull at a phone dump, SD card copy, external SSD, or nested family archive. It reads the folder tree and builds the workspace from files already on disk.",
      tag: "Folder",
      visual: "folder"
    },
    {
      title: "Cull at keyboard speed",
      body: "Use X to reject, P to pick, 1-5 for stars, color labels, tags, and filters. Grid, details, focus, and filmstrip views cover different review moments.",
      tag: "Keys",
      visual: "keys"
    },
    {
      title: "Handle modern mixed media",
      body: "Photos, RAWs, and videos sit in one review lane. HEVC clips from phones, DJI Osmo Pocket, drones, or 4K60 sources get poster frames, scrubbing, and system-player fallback when needed.",
      tag: "Video",
      visual: "video"
    },
    {
      title: "Sweep rejects without panic",
      body: "Originals are untouched during review. When you sweep rejects, they go to an in-app Trash or OS bin, with restore available before permanent delete.",
      tag: "Trash",
      visual: "trash"
    }
  ],
  downloadTitle: "Stable first, beta when curious",
  downloadIntro: "The public fox-cull stable build is the clean culling baseline. Newer experiments live in beta and in the more active FoxCull Codex line before they become stable.",
  panels: [
    {
      title: "Folder-native",
      body: "No import ceremony. Ratings and cache live in a clear _FoxCull folder per drive, so external SSDs can carry their culling state with them."
    },
    {
      title: "Made for real clutter",
      body: "Phone JPEGs, RAW files, video clips, subfolders, capture-date grouping, and reject filters are treated as the normal case, not edge cases."
    },
    {
      title: "Open-source Lightroom alternative",
      body: "It focuses on the first-pass library job: find keepers, reject clutter, preview media, and sweep safely without subscription bloat."
    }
  ],
  setupTitle: "How to start culling",
  setupIntro: "Install, choose a folder, move through the grid, mark rejects, then sweep only after the review pass feels right.",
  setup: [
    { title: "Install fox-cull", body: "Use the stable build for your operating system." },
    { title: "Choose the folder", body: "Open a drive, camera-card copy, phone dump, or archive folder. Subfolders can be included." },
    { title: "Rate, pick, reject", body: "Use the keyboard, filter down, and use focus mode or filmstrip when a closer look matters." },
    { title: "Sweep safely", body: "Move rejects to the recoverable Trash when you are ready. Originals are not modified during review." }
  ],
  footer: "Open-source first-pass media culling for photos, RAWs, and videos."
};
