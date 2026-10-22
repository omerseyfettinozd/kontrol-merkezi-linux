-- Lua counterpart of r9t-session.conf for Hyprland 0.55+ (hyprland.lua is loaded
-- instead of hyprland.conf when it exists; .conf support ends in 0.57).
-- Put the hl.exec_cmd line in your own hl.on("hyprland.start", ...) block, where
-- `r9tctl desktop` finds and swaps it, or require this file from hyprland.lua:
--   require("/usr/share/r9t-studio/hyprland/r9t-session.lua")
hl.on("hyprland.start", function()
    hl.exec_cmd("systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE && systemctl --user start r9t-session.service")
end)
