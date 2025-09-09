use std::net::SocketAddr;

const ASCII_LOGO: &str = concat!(
    "\n",
    "       =*##*-            -*##*=                       \n",
    "   *##-##-   =:        .+   :##-##*.                  \n",
    " =######+++##-          :##+++######=                 \n",
    "-##########      ....     .##########=                \n",
    "-=####-.*@@@*=-:.    .:-=*@@@@######+-                \n",
    "+##*+##: @%@@@@%#******#%@@@@#@+*##++##*              \n",
    "####-    @.:+                 @    :####              \n",
    "=###-+   @+ -                .@   +-*##+              \n",
    " .=####  @%@@@+:        :+@@@*@  *###=.               \n",
    "  :####: @.                   @ :####:                \n",
    "  .  =#: @.+#-                @ :#=                   \n",
    "+   .    @@:                 @@   ..  +               \n",
    " +*  :+: @. *@@@@@@@@@@@@@@#  @ .-  +*.               \n",
    "=   +++. @.:*.                @ .++-  .=              \n",
    "  #.     @# :                :@     #:                \n",
    " * :*#-*#-*@@%+:        .=#@@@##=*#= +=               \n",
    ":  #  *#=:      :-====-:     .-##  #  *               \n",
    "   #  +:#####             *####--  #  .               \n",
    "   *   #####+.#+       =#=+#####.  *                  \n",
    "    :  :####*  :*     *=  *####=  :                   \n",
    "         =#####.       .*####+                        \n",
);

pub fn build_banner(local_addr: SocketAddr) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let bits = (std::mem::size_of::<usize>() * 8) as u32;
    let mode = "standalone";
    let port = local_addr.port();
    let pid = std::process::id();

    format!(
        "{logo}\nRustCache Open Source  v{version} ({profile}) {bits}-bit\nRunning in {mode} mode\nPort: {port}\nPID: {pid}\n",
        logo = ASCII_LOGO,
        version = version,
        profile = profile,
        bits = bits,
        mode = mode,
        port = port,
        pid = pid
    )
}
