# Sherpa: улучшенная оболочка (zsh) в стиле Kali. Файлы лежат только в ~/.sherpa, ~/.zshrc не затрагивается.
[[ $LANG == *UTF-8* || $LC_ALL == *UTF-8* ]] || export LC_CTYPE=C.UTF-8

HISTFILE="$HOME/.sherpa/history_zsh"
HISTSIZE=10000; SAVEHIST=10000
setopt autocd interactivecomments magicequalsubst nonomatch notify numericglobsort promptsubst
setopt hist_expire_dups_first hist_ignore_dups hist_ignore_space hist_verify share_history
WORDCHARS='_-'
PROMPT_EOL_MARK=''

# --- клавиши ---
bindkey -e
bindkey '^[[A' up-line-or-search
bindkey '^[[B' down-line-or-search
bindkey '^[[1;5C' forward-word
bindkey '^[[1;5D' backward-word
bindkey '^[[H' beginning-of-line
bindkey '^[[F' end-of-line
bindkey '^[[3~' delete-char
bindkey '^[[Z' reverse-menu-complete

# --- автодополнение с меню ---
autoload -Uz compinit
compinit -u -d "${ZDOTDIR:-$HOME/.sherpa/shell}/.zcompdump"
zstyle ':completion:*' menu select
zstyle ':completion:*' auto-description 'указать: %d'
zstyle ':completion:*' format ' %F{yellow}-- %d --%f'
zstyle ':completion:*' group-name ''
zstyle ':completion:*' list-colors ${(s.:.)LS_COLORS}
zstyle ':completion:*' matcher-list 'm:{a-zA-Z}={A-Za-z}'
zstyle ':completion:*' use-compctl false
zstyle ':completion:*' verbose true
zstyle ':completion:*:kill:*' command 'ps -u $USER -o pid,%cpu,tty,cputime,cmd'

# --- цвета ---
if [[ -x /usr/bin/dircolors ]]; then eval "$(dircolors -b)"; fi
export LS_COLORS="${LS_COLORS:-di=34:ln=36:ex=32:so=35:pi=33}"
alias ls='ls --color=auto'
alias grep='grep --color=auto'
alias egrep='grep -E --color=auto'
alias fgrep='grep -F --color=auto'
alias diff='diff --color=auto'
alias ll='ls -l'
alias la='ls -A'
alias l='ls -CF'
export LESS_TERMCAP_md=$'\E[1;34m' LESS_TERMCAP_us=$'\E[4;36m' LESS_TERMCAP_ue=$'\E[0m' LESS_TERMCAP_me=$'\E[0m'

# --- промпт Kali ---
# ┌──(user㉿host)-[~/path]
# └─$
autoload -U colors && colors
if [[ $EUID -eq 0 ]]; then _c=red; _u='💀'; _p='#'; else _c=blue; _u='㉿'; _p='$'; fi
PROMPT=$'%F{'$_c$'}┌──(%B%F{'$_c$'}%n'$_u$'%m%b%F{'$_c$'})-[%B%F{reset}%(6~.%-1~/…/%4~.%5~)%b%F{'$_c$'}]\n└─%B%F{'$_c$'}'$_p$'%b%F{reset} '
unset _c _u _p
precmd() { print -Pn '\e]0;%n@%m: %~\a'; }

# --- плагины ---
_sg="${ZDOTDIR:-$HOME/.sherpa/shell}/plugins"
ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE='fg=8'
ZSH_AUTOSUGGEST_STRATEGY=(history completion)
[[ -f $_sg/zsh-autosuggestions/zsh-autosuggestions.zsh ]] && source "$_sg/zsh-autosuggestions/zsh-autosuggestions.zsh"
ZSH_HIGHLIGHT_HIGHLIGHTERS=(main brackets)
typeset -A ZSH_HIGHLIGHT_STYLES
ZSH_HIGHLIGHT_STYLES[command]='fg=green,bold'
ZSH_HIGHLIGHT_STYLES[builtin]='fg=green,bold'
ZSH_HIGHLIGHT_STYLES[alias]='fg=green,bold'
ZSH_HIGHLIGHT_STYLES[unknown-token]='fg=red,bold'
ZSH_HIGHLIGHT_STYLES[path]='fg=cyan,underline'
# подсветка должна подключаться последней
[[ -f $_sg/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh ]] && source "$_sg/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh"
unset _sg
