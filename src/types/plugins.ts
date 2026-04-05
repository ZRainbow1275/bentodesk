// BentoDesk Plugin types — mirrors Rust data model exactly

export type PluginType = "theme" | "widget" | "organizer";

export interface InstalledPlugin {
  id: string;
  name: string;
  version: string;
  plugin_type: PluginType;
  author: string;
  description: string;
  enabled: boolean;
  installed_at: string;
  install_path: string;
}
