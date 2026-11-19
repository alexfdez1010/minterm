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
