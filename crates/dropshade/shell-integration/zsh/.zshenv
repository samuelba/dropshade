# Dropshade shell integration for zsh.
#
# Dropshade points ZDOTDIR at this directory so zsh reads this file first. It puts ZDOTDIR back,
# loads your own .zshenv, and in interactive shells reports prompts and commands to the terminal
# with OSC 133 marks:
#   A  the prompt starts            B  the command line starts
#   C  the command's output starts  D  the command finished, with its exit status
# Turn it off with `shell_integration = false` in the [terminal] section of the config.

if [[ -n ${DROPSHADE_ZDOTDIR+set} ]]; then
    ZDOTDIR=$DROPSHADE_ZDOTDIR
    unset DROPSHADE_ZDOTDIR
else
    unset ZDOTDIR
fi

if [[ -r ${ZDOTDIR:-$HOME}/.zshenv ]]; then
    builtin source "${ZDOTDIR:-$HOME}/.zshenv"
fi

if [[ -o interactive && -z ${__dropshade_integrated-} ]]; then
    typeset -g __dropshade_integrated=1
    autoload -Uz add-zsh-hook

    __dropshade_precmd() {
        local ret=$?
        builtin print -n "\e]133;D;${ret}\a"
        if [[ $PROMPT != *'133;A'* ]]; then
            PROMPT=$'%{\e]133;A\a%}'"$PROMPT"$'%{\e]133;B\a%}'
        fi
        return $ret
    }

    __dropshade_preexec() {
        builtin print -n "\e]133;C\a"
    }

    add-zsh-hook precmd __dropshade_precmd
    add-zsh-hook preexec __dropshade_preexec
fi
