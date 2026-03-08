# Module factory — receives { hmHelpers } from flake.nix
{ hmHelpers }:
{
  lib,
  config,
  pkgs,
  ...
}:
with lib;
let
  cfg = config.blackmatter.components.hikyaku;

  yamlConfig =
    pkgs.writeText "hikyaku.yaml" (lib.generators.toYAML { } cfg.settings);
in
{
  options.blackmatter.components.hikyaku = {
    enable = mkEnableOption "Hikyaku — GPU-rendered terminal email client";

    package = mkOption {
      type = types.package;
      default = pkgs.hikyaku;
      description = "The hikyaku package to use.";
    };

    settings = mkOption {
      type = types.attrs;
      default = { };
      description = ''
        Configuration written to `~/.config/hikyaku/hikyaku.yaml`.
        Accepts any attrs that serialize to valid hikyaku YAML config.
        Shikumi loads: defaults → env vars (HIKYAKU_*) → this file.
      '';
      example = {
        theme = {
          name = "nord";
        };
        accounts = {
          personal = {
            provider = "gmail";
            address = "you@gmail.com";
            oauth2 = true;
            password_command = "cat /run/secrets/gmail-token";
          };
          work = {
            provider = "gmail_workspace";
            address = "you@company.com";
            oauth2 = true;
            password_command = "cat /run/secrets/work-gmail-token";
          };
          proton = {
            provider = "protonmail";
            address = "you@proton.me";
            imap_host = "127.0.0.1";
            imap_port = 1143;
            smtp_host = "127.0.0.1";
            smtp_port = 1025;
            password_command = "cat /run/secrets/proton-bridge-password";
          };
        };
        rendering = {
          graphics_protocol = "auto";
          inline_images = true;
        };
        keybindings = {
          quit = "q";
          down = "j";
          up = "k";
          compose = "c";
        };
      };
    };

    scripting = {
      initScript = mkOption {
        type = types.lines;
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

          // Tag invoices
          on_receive("subject:invoice", || {
            tag("finance");
            notify("Invoice received", subject());
          });
        '';
      };

      extraScripts = mkOption {
        type = types.attrsOf types.lines;
        default = { };
        description = ''
          Additional Rhai scripts written to `~/.config/hikyaku/scripts/<name>.rhai`.
        '';
        example = {
          "auto-label" = ''
            log("auto-label rules loaded");
          '';
        };
      };

      hotReload = mkOption {
        type = types.bool;
        default = true;
        description = "Enable hot-reload of Rhai scripts on file changes.";
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    # Install the package
    {
      home.packages = [ cfg.package ];
    }

    # YAML configuration (shikumi-based, hot-reloaded)
    (mkIf (cfg.settings != { }) {
      xdg.configFile."hikyaku/hikyaku.yaml".source = yamlConfig;
    })

    # Rhai init script
    (mkIf (cfg.scripting.initScript != "") {
      xdg.configFile."hikyaku/init.rhai".text = cfg.scripting.initScript;
    })

    # Extra Rhai scripts
    (mkIf (cfg.scripting.extraScripts != { }) {
      xdg.configFile = mapAttrs' (
        name: content: nameValuePair "hikyaku/scripts/${name}.rhai" { text = content; }
      ) cfg.scripting.extraScripts;
    })

    # Scripting config wired into settings
    (mkIf (cfg.scripting.initScript != "" || cfg.scripting.extraScripts != { }) {
      blackmatter.components.hikyaku.settings = lib.mkDefault {
        scripting = {
          init_script = "~/.config/hikyaku/init.rhai";
          script_dirs = [ "~/.config/hikyaku/scripts" ];
          hot_reload = cfg.scripting.hotReload;
        };
      };
    })
  ]);
}
