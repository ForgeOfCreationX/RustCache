use std::net::SocketAddr;

const ASCII_LOGO: &str = concat!(
    "\n",
    "       =*##*-            -*##*=                          \n",
    "   *##-##-   =:        .+   :##-##*.                     \n",
    " =######+++##-          :##+++######=                    \n",
    "-##########      ....     .##########=                   \n",
    "-=####-.*@@@*=-:.    .:-=*@@@@######+-       RustCache Open Source\n",
    "+##*+##: @%@@@@%#******#%@@@@#@+*##++##*     {build_info} \n",
    "####-    @.:+                 @    :####     Port: {port}\n",
    "=###-+   @+ -                .@   +-*##+     PID: {pid}\n",
    " .=####  @%@@@+:        :+@@@*@  *###=.                  \n",
    "  :####: @.                   @ :####:                   \n",
    "  .  =#: @.+#-                @ :#=                      \n",
    "+   .    @@:                 @@   ..  +                  \n",
    " +*  :+: @. *@@@@@@@@@@@@@@#  @ .-  +*.                  \n",
    "=   +++. @.:*.                @ .++-  .=                 \n",
    "  #.     @# :                :@     #:                   \n",
    " * :*#-*#-*@@%+:        .=#@@@##=*#= +=                  \n",
    ":  #  *#=:      :-====-:     .-##  #  *                  \n",
    "   #  +:#####             *####--  #  .                  \n",
    "   *   #####+.#+       =#=+#####.  *                     \n",
    "    :  :####*  :*     *=  *####=  :                      \n",
    "         =#####.       .*####+                           \n",
);

pub fn build_banner(local_addr: SocketAddr) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let bits = (std::mem::size_of::<usize>() * 8) as u32;
    let port = local_addr.port();
    let pid = std::process::id();

    let build_info = format!("v{} ({}) {}-bit", version, profile, bits);

    let ascii_art = ASCII_LOGO
        .replace("{port}", &port.to_string())
        .replace("{build_info}", &build_info.to_string())
        .replace("{pid}", &pid.to_string());

    format!("{}\n",ascii_art)
}