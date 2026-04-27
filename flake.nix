{
  description = "Hikyaku — GPU-rendered terminal email client with MCP and scripting";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    crate2nix.url = "github:nix-community/crate2nix";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crate2nix,
    flake-utils,
    substrate,
  }:
    (import "${substrate}/lib/rust-tool-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils;
    }) {
      toolName = "hikyaku";
      src = self;
      repo = "pleme-io/hikyaku";

      # Migration to substrate module-trio + shikumiTypedGroups.
      # See kekkai (2fc3c84) and hikki (ec91444) for canonical templates.
      # hikyaku demonstrates: freeform shikumi YAML (no typed groups) +
      # bespoke Rhai scripting surface (init.rhai + scripts/*.rhai)
      # written via extraHmConfigFn.
      module = {
        description = "Hikyaku — GPU-rendered terminal email client with MCP and scripting";
        hmNamespace = "blackmatter.components";

        # Shikumi YAML at ~/.config/hikyaku/hikyaku.yaml. The legacy
        # module exposed `settings` as a freeform `types.attrs` —
        # withShikumiConfig provides exactly that shape (services.hikyaku.
        # settings as types.attrs), so consumers can author any tree
        # of YAML keys directly. No typed groups: the email-client
        # config schema (theme/accounts/keybindings) is too varied to
        # pin down to typed primitives at this layer.
        withShikumiConfig = true;

        # Bespoke options: scripting surface for Rhai scripts.
        extraHmOptions = {
          scripting = {
            initScript = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.lines;
              default = "";
              description = ''
                Contents of `~/.config/hikyaku/init.rhai`.
                Main Rhai script loaded on startup.
              '';
              example = ''
                log("hikyaku init.rhai loaded");

                // Auto-archive GitHub notifications after reading
                on_receive("from:notifications@github.com", || {
                  if is_read() {
                    move_to("Archive");
                  }
                });
              '';
            };

            extraScripts = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.attrsOf nixpkgs.lib.types.lines;
              default = { };
              description = ''
                Additional Rhai scripts written to
                `~/.config/hikyaku/scripts/<name>.rhai`.
              '';
            };

            hotReload = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.bool;
              default = true;
              description = "Enable hot-reload of Rhai scripts on file changes.";
            };
          };
        };

        # Wire the Rhai scripts (init.rhai + scripts/*.rhai) and merge
        # the scripting block into the YAML payload. Same shape as the
        # legacy module's xdg.configFile + scripting-settings merge.
        extraHmConfigFn = { cfg, lib, ... }:
          lib.mkMerge [
            (lib.mkIf (cfg.scripting.initScript != "") {
              xdg.configFile."hikyaku/init.rhai".text = cfg.scripting.initScript;
            })

            (lib.mkIf (cfg.scripting.extraScripts != { }) {
              xdg.configFile = lib.mapAttrs' (
                name: content: lib.nameValuePair "hikyaku/scripts/${name}.rhai" {
                  text = content;
                }
              ) cfg.scripting.extraScripts;
            })

            (lib.mkIf (cfg.scripting.initScript != "" || cfg.scripting.extraScripts != { }) {
              services.hikyaku.settings = lib.mkDefault {
                scripting = {
                  init_script = "~/.config/hikyaku/init.rhai";
                  script_dirs = [ "~/.config/hikyaku/scripts" ];
                  hot_reload = cfg.scripting.hotReload;
                };
              };
            })
          ];
      };
    };
}
