# ZLE displays the suffix outside BUFFER. Enter therefore submits only the
# user's input, and full-screen programs never see this integration.
_minterm_suggest_clear() {
  emulate -L zsh
  region_highlight=("${(@)region_highlight:#* memo=minterm-suggestion}")
  if [[ ${POSTDISPLAY-} == "${_minterm_suffix-}" ]]; then POSTDISPLAY=''; fi
  _minterm_suffix=''
  return 0
}

_minterm_suggest_lookup() {
  emulate -L zsh
  local prefix=$1 entry
  local -i event
  # HISTCMD refers to the line being edited. Look only in this shell's recent
  # in-memory history; a large HISTFILE never puts disk I/O on the typing path.
  for ((event=HISTCMD-1; event>0 && event>=HISTCMD-256; --event)); do
    entry=${history[$event]-}
    if [[ $entry == "$prefix"?* && $entry != *[[:cntrl:]]* ]]; then
      REPLY=$entry
      return
    fi
  done
  REPLY=''
  # Complete a bare command from zsh's command tables. Argument completion
  # remains with the user's Tab widget and its command-specific rules.
  [[ $prefix == [[:alnum:]_]* && $prefix != *[^[:alnum:]_.+-]* ]] || return 0
  if (( ${+commands[$prefix]} || ${+aliases[$prefix]} || ${+builtins[$prefix]} || ${+functions[$prefix]} )); then return 0; fi
  local pattern=${(b)prefix}'*'
  local -a names
  names=( ${(k)aliases[(I)${~pattern}]} ${(k)functions[(I)${~pattern}]}
          ${(k)builtins[(I)${~pattern}]} ${(k)commands[(I)${~pattern}]} )
  for entry in ${(ou)names}; do
    if [[ $entry != *[[:cntrl:]]* && $entry != _minterm_* ]]; then REPLY=$entry; break; fi
  done
  return 0
}

_minterm_suggest_redraw() {
  emulate -L zsh
  _minterm_suggest_clear
  [[ -z $POSTDISPLAY && ${CONTEXT-} == start && ${KEYMAP-} == (main|emacs|viins) &&
     -n $BUFFER && $BUFFER != ' '* && $BUFFER != *[[:cntrl:]]* &&
     $CURSOR == ${#BUFFER} && ${REGION_ACTIVE:-0} == 0 ]] || return 0

  if [[ $BUFFER != "${_minterm_prefix-}" || $HISTCMD != "${_minterm_event-}" ]]; then
    local REPLY
    _minterm_suggest_lookup "$BUFFER"
    _minterm_candidate=$REPLY
    _minterm_prefix=$BUFFER
    _minterm_event=$HISTCMD
  fi
  [[ ${_minterm_candidate-} == "$BUFFER"?* ]] || return 0
  _minterm_suffix=${_minterm_candidate:$#BUFFER}
  POSTDISPLAY=$_minterm_suffix
  region_highlight+=("$#BUFFER $#_minterm_candidate fg=8 memo=minterm-suggestion")
  return 0
}

_minterm_suggest_accept() {
  emulate -L zsh
  if [[ -n ${_minterm_suffix-} && $POSTDISPLAY == "$_minterm_suffix" &&
        $BUFFER == "$_minterm_prefix" && $CURSOR == ${#BUFFER} ]]; then
    LBUFFER+=$_minterm_suffix
    _minterm_suggest_clear
  else
    zle "${WIDGET#_minterm_suggest_}"
  fi
}

_minterm_suggest_install() {
  emulate -L zsh
  [[ ${MINTERM_AUTOSUGGEST:-1} != 0 && -o zle ]] || return 0
  # Leave an existing autosuggestion provider and custom right-arrow widgets in charge.
  (( ${+widgets[autosuggest-accept]} || ${+functions[_zsh_autosuggest_start]} )) && return 0
  zmodload zsh/parameter || return 0
  autoload -Uz add-zle-hook-widget is-at-least
  # Tagged highlights survive ZLE adjusting their offsets as the user types.
  is-at-least 5.9 || return 0
  zle -N _minterm_suggest_forward-char _minterm_suggest_accept
  zle -N _minterm_suggest_vi-forward-char _minterm_suggest_accept
  zle -N _minterm_suggest_clear
  zle -N _minterm_suggest_redraw
  add-zle-hook-widget line-pre-redraw _minterm_suggest_redraw
  add-zle-hook-widget line-finish _minterm_suggest_clear
  local map key binding
  for map in emacs viins; do
    for key in '^[[C' '^[OC'; do
      binding=$(bindkey -M "$map" "$key")
      if [[ ${binding##* } == (forward-char|vi-forward-char) ]]; then
        bindkey -M "$map" "$key" "_minterm_suggest_${binding##* }"
      fi
    done
  done
}
