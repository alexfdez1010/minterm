# Shared by the Bash and zsh prompts. Read Git's small HEAD file directly so
# typing a command never starts a Git process or walks the working tree.
_minterm_git_branch() {
    local dir marker head pointer value
    _minterm_branch=
    if [ "${_minterm_logical_pwd-}" != "$PWD" ]; then
        _minterm_logical_pwd=$PWD
        # The sentinel preserves trailing newlines in a legal directory name.
        if _minterm_physical_pwd=$(builtin pwd -P 2>/dev/null && builtin printf .); then
            _minterm_physical_pwd=${_minterm_physical_pwd%$'\n.'}
        else
            _minterm_physical_pwd=$PWD
        fi
    fi
    dir=${_minterm_physical_pwd:-$PWD}
    marker=${GIT_DIR-}
    if [ -z "$marker" ]; then
        while :; do
            if [ -d "$dir/.git" ] || [ -f "$dir/.git" ]; then
                marker=$dir/.git
                break
            fi
            if [ -f "$dir/HEAD" ] && [ -d "$dir/objects" ] && [ -d "$dir/refs" ]; then
                marker=$dir
                break
            fi
            [ "$dir" != / ] || return 0
            dir=${dir%/*}
            dir=${dir:-/}
        done
    fi
    if [ -f "$marker" ]; then
        IFS= builtin read -r pointer < "$marker" || return 0
        case "$pointer" in
            'gitdir: /'*) marker=${pointer#gitdir: } ;;
            'gitdir: '*) marker=$dir/${pointer#gitdir: } ;;
            *) return 0 ;;
        esac
    fi
    head=$marker/HEAD
    [ -f "$head" ] && [ -r "$head" ] || return 0
    IFS= builtin read -r value < "$head" || return 0
    case "$value" in
        'ref: refs/heads/'*) _minterm_branch=${value#ref: refs/heads/} ;;
        ''|*[!0-9a-fA-F]*) ;;
        *) if [ "${#value}" -eq 40 ] || [ "${#value}" -eq 64 ]; then
               _minterm_branch=${value:0:7}
           fi ;;
    esac
    return 0
}
