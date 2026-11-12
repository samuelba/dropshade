# Dropshade shell integration for bash.
#
# Dropshade starts bash with `--rcfile` pointing at this file. It loads ~/.bashrc as bash would
# have, then reports prompts and commands to the terminal with OSC 133 marks:
#   A  the prompt starts            B  the command line starts
#   C  the command's output starts  D  the command finished, with its exit status
# Turn it off with `shell_integration = false` in the [terminal] section of the config.

if [[ -f ~/.bashrc ]]; then
    builtin source ~/.bashrc
fi

if [[ $- == *i* && -z ${__dropshade_integrated-} ]]; then
    __dropshade_integrated=1

    # Runs first after each command, while $? is still the command's status, and hands that
    # status on to the hooks after it. (PIPESTATUS shrinks to that one status.)
    __dropshade_precmd() {
        local ret=$?
        builtin printf '\e]133;D;%s\a' "$ret"
        return "$ret"
    }

    # Runs last, after prompt themes such as starship have set PS1 for this prompt.
    __dropshade_prompt() {
        local ret=$?
        if [[ $PS1 != *'133;A'* ]]; then
            PS1='\[\e]133;A\a\]'"$PS1"'\[\e]133;B\a\]'
        fi
        if [[ ${PS0-} != *'133;C'* ]]; then
            PS0="${PS0-}"'\e]133;C\a'
        fi
        return "$ret"
    }

    if [[ ${PROMPT_COMMAND@a} == *a* ]]; then
        PROMPT_COMMAND=(__dropshade_precmd "${PROMPT_COMMAND[@]}" __dropshade_prompt)
    else
        PROMPT_COMMAND=$'__dropshade_precmd\n'"${PROMPT_COMMAND-}"$'\n__dropshade_prompt'
    fi
fi
