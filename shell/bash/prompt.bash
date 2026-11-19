# Loaded once by PROMPT_COMMAND, after Bash finishes its normal login sequence.
builtin source "$MINTERM_SHELL_INTEGRATION/git.sh"

__minterm_bash_prompt() {
    local status=$1 color=32
    if [[ ${MINTERM_PROMPT-1} == 0 ]]; then
        return "$status"
    fi

    case $PWD in
        "$HOME") __minterm_prompt_dir='~' ;;
        /) __minterm_prompt_dir=/ ;;
        *) __minterm_prompt_dir=${PWD##*/} ;;
    esac
    __minterm_prompt_dir=${__minterm_prompt_dir//[[:cntrl:]]/?}
    _minterm_git_branch
    __minterm_prompt_branch=${_minterm_branch//[[:cntrl:]]/?}

    # Bash decodes prompt escapes before expanding parameters. Keep path and
    # branch data in variables so their content never becomes prompt syntax.
    local directory='${__minterm_prompt_dir}' branch='${__minterm_prompt_branch}'
    if ! shopt -q promptvars && [[ ! -o posix ]]; then
        # With expansion disabled, only backslashes need quoting for PS1.
        directory=${__minterm_prompt_dir//\\/\\\\}
        branch=${__minterm_prompt_branch//\\/\\\\}
    fi

    PS1='\[\e[34m\]'${directory}' '
    if [[ -n $__minterm_prompt_branch ]]; then
        PS1+='\[\e[32m\]⎇ '${branch}' '
    fi
    (( status == 0 )) || color=33
    PS1+='\[\e['${color}'m\]❯\[\e[0m\] '
    return "$status"
}

# Apple's Bash 3.2 Readline has no post-display buffer. Shift-Tab inserts and
# cycles the native completion candidates; keep the user's Tab and arrow keys.
if [[ $- == *i* && ${MINTERM_AUTOSUGGEST:-1} != 0 ]]; then
    for __minterm_map in emacs-standard vi-insert; do
        __minterm_bindings=$(builtin bind -m "$__minterm_map" -p; builtin bind -m "$__minterm_map" -s)
        case $'\n'$__minterm_bindings in
            *$'\n"\\e[Z":'*) ;;
            *) builtin bind -m "$__minterm_map" '"\e[Z": menu-complete' ;;
        esac
    done
    unset __minterm_map __minterm_bindings
fi
