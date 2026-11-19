# Return startup to the user's normal ZDOTDIR, including changes made by .zshenv.
# System and user login/interactive files then run in their original order.
if (( ${+MINTERM_ORIGINAL_ZDOTDIR} )); then
  export ZDOTDIR="$MINTERM_ORIGINAL_ZDOTDIR"
  unset MINTERM_ORIGINAL_ZDOTDIR
else
  unset ZDOTDIR
fi

if [[ -o interactive ]] && builtin source "$MINTERM_SHELL_INTEGRATION/git.sh"; then
  _minterm_zsh_precmd() {
    emulate -L zsh
    local _minterm_branch
    _minterm_git_branch
    # %v inserts data after prompt substitution and percent expansion. Branch
    # names stay literal with PROMPT_SUBST, PROMPT_BANG and custom zsh themes.
    psvar[$_minterm_branch_slot]=$_minterm_branch
    PROMPT='%B%F{blue}%1~%f%b'
    if [[ -n $_minterm_branch ]]; then
      PROMPT+=" %F{green}⎇ %${_minterm_branch_slot}v%f"
    fi
    PROMPT+=' %(?.%F{green}.%F{yellow})❯%f '
  }

  _minterm_zsh_install() {
    # zsh's scheduler runs after startup and before the first interactive prompt.
    if [[ ${MINTERM_PROMPT:-1} != 0 && -o promptpercent ]]; then
      typeset -gi _minterm_branch_slot=$(( ${#psvar} + 1 ))
      precmd_functions+=(_minterm_zsh_precmd)
      _minterm_zsh_precmd
    fi
    if [[ ${MINTERM_AUTOSUGGEST:-1} != 0 ]]; then
      builtin source "$MINTERM_SHELL_INTEGRATION/zsh/suggest.zsh" && _minterm_suggest_install
    fi
    unfunction _minterm_zsh_install
  }
  builtin zmodload zsh/sched && builtin sched +0 _minterm_zsh_install
fi

if [[ -r ${ZDOTDIR-$HOME}/.zshenv ]]; then
  builtin source "${ZDOTDIR-$HOME}/.zshenv"
fi
