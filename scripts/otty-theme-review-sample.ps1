# Fixed terminal output: same cells remain on screen while themes change.
$esc = [char]27
$reset = "$esc[0m"
Write-Host ''
Write-Host '  HITTY / OTTY THEME COMPARISON' -ForegroundColor Cyan
Write-Host '  Settings > Appearance > Theme: search One Light, Dracula, Nord, Tokyo Night.'
Write-Host '  Original + "- otty" variants share the same Hitty renderer and live session.'
Write-Host ''
$normal = ''; $bright = ''
for ($i = 0; $i -lt 8; $i++) {
    $normal += "$esc[$(30 + $i)m$('{0,-10}' -f "ANSI $i")$reset"
    $bright += "$esc[$(90 + $i)m$('{0,-10}' -f "ANSI $($i + 8)")$reset"
}
Write-Host $normal
Write-Host $bright
Write-Host "$esc[32m  success$reset  $esc[31merror$reset  $esc[33mwarning$reset  $esc[36minfo$reset  $esc[2mdim hint$reset"
Write-Host ''
Write-Host "$esc[39;48;2;242;242;242m  Cached light RGB input: text / placeholder / Enter to confirm  $reset"
Write-Host "$esc[39;48;2;65;69;76m  Cached dark RGB input: text / placeholder / Enter to confirm   $reset"
Write-Host "$esc[39;48;5;236m  Indexed 236 input: try switching light > dark > light         $reset"
Write-Host "$esc[2;39;48;2;242;242;242m  SGR dim input hint: check contrast after switching             $reset"
Write-Host "$esc[38;2;77;118;111;48;2;218;251;225m  + Added line / explicit RGB diff                               $reset"
Write-Host "$esc[38;2;104;109;124;48;2;255;235;233m  - Removed line / explicit RGB diff                             $reset"
Write-Host ''
Write-Host '  Select text; search for "input"; compare caret and dim text.'
Write-Host '  Run codex or claude here when ready. No Agent request is sent automatically.'
Write-Host '  Toggle Readable terminal colors to compare adapted versus literal colors.'
Write-Host '  Glass variants follow the theme''s declared surfaces and native materials.'
Write-Host ''
