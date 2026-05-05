project('tunecraft', 'rust',
  version: '5.0',
  meson_version: '>= 0.59.0',
  default_options: ['warning_level=2', 'werror=false'],
)

i18n = import('i18n')
gnome = import('gnome')

subdir('po')

prefix = get_option('prefix')
bindir = get_option('bindir')
datadir = get_option('datadir')

# Cargo build configuration
cargo = find_program('cargo', required: true)
cargo_manifest = meson.project_source_root() / 'Cargo.toml'
cargo_target_dir = meson.build_root() / 'target'

# Build the Rust binary with cargo
# The binary crate is named 'tunecraft-ui' in Cargo.toml
# NOTE: install: false here — the install_data block below handles
# installation with the correct rename to 'tunecraft'.
cargo_build = custom_target(
  'tunecraft-build',
  input: cargo_manifest,
  output: 'tunecraft-ui',
  command: [
    cargo, 'build', '--release',
    '--manifest-path', '@INPUT@',
    '--target-dir', cargo_target_dir,
  ],
  install: false,
)

# Install the compiled binary from cargo's output directory
# Cargo outputs the binary as 'tunecraft-ui' (the crate name)
install_data(
  cargo_target_dir / 'release' / 'tunecraft-ui',
  install_dir: bindir,
  install_tag: 'bin',
  rename: 'tunecraft',  # Install as 'tunecraft' for user convenience
)

# Desktop file
desktop_file = i18n.merge_file(
  input: 'tunecraft.desktop.in',
  output: 'org.tunecraft.Tunecraft.desktop',
  type: 'desktop',
  po_dir: 'po',
  install: true,
  install_dir: join_paths(datadir, 'applications'),
)

# AppStream metadata
appstream_file = i18n.merge_file(
  input: 'tunecraft.appdata.xml',
  output: 'org.tunecraft.Tunecraft.appdata.xml',
  type: 'appstream',
  po_dir: 'po',
  install: true,
  install_dir: join_paths(datadir, 'metainfo'),
)

# Install icons (if they exist)
icon_dir = join_paths(datadir, 'icons', 'hicolor')
scalable_icon = meson.project_source_root() / 'data' / 'icons' / 'scalable' / 'org.tunecraft.Tunecraft.svg'
symbolic_icon = meson.project_source_root() / 'data' / 'icons' / 'symbolic' / 'org.tunecraft.Tunecraft-symbolic.svg'

if run_command('test', '-f', scalable_icon, check: false).returncode() == 0
  install_data(
    scalable_icon,
    install_dir: join_paths(icon_dir, 'scalable', 'apps'),
  )
endif

if run_command('test', '-f', symbolic_icon, check: false).returncode() == 0
  install_data(
    symbolic_icon,
    install_dir: join_paths(icon_dir, 'symbolic', 'apps'),
  )
endif

# Install sample config
install_data(
  'config.toml.sample',
  install_dir: join_paths(datadir, 'doc', 'tunecraft'),
)

# Install AppStream screenshots (for Flathub)
screenshot_dir = meson.project_source_root() / 'data' / 'screenshots'
screenshot_file = screenshot_dir / 'main-window.png'

if run_command('test', '-f', screenshot_file, check: false).returncode() == 0
  install_data(
    screenshot_file,
    install_dir: join_paths(datadir, 'appdata', 'xml', 'screenshots'),
  )
endif

# Validate desktop file
if find_program('desktop-file-validate', required: false).found()
  test('validate-desktop', find_program('desktop-file-validate'),
    args: [desktop_file.full_path()])
endif

# Validate appstream metadata
if find_program('appstream-util', required: false).found()
  test('validate-appstream', find_program('appstream-util'),
    args: ['validate-relax', appstream_file.full_path()])
endif

# Run cargo test as a meson test
test('cargo-test', cargo,
  args: ['test', '--manifest-path', cargo_manifest, '--target-dir', cargo_target_dir],
  workdir: meson.project_source_root(),
  timeout: 300,
)
