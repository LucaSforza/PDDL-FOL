if exists('b:current_syntax')
  finish
endif

syntax case ignore

syntax keyword folplanSection problem types objects predicates init goal pre effect
syntax keyword folplanSection action nextgroup=folplanAction skipwhite
syntax keyword folplanQuantifier forall exists
syntax keyword folplanBoolean true false
syntax keyword folplanLogic and or not implies
syntax match folplanPredicate /\<[A-Za-z][A-Za-z0-9_-]*\ze\s*(/
syntax match folplanAction /[A-Za-z][A-Za-z0-9_-]*/ contained
syntax match folplanOperator /[!&|=]\|->/
syntax match folplanDelimiter /[{}(),;:.]/
syntax match folplanComment /\/\/.*/ contains=@Spell

highlight folplanSection guifg=#FF5FA2 gui=bold ctermfg=205 cterm=bold
highlight folplanAction guifg=#FFE66D gui=bold ctermfg=221 cterm=bold
highlight folplanPredicate guifg=#43E7FF ctermfg=51
highlight folplanQuantifier guifg=#D090FF gui=bold ctermfg=177 cterm=bold
highlight folplanBoolean guifg=#B9F87A ctermfg=155
highlight folplanLogic guifg=#FFAF5F ctermfg=215
highlight folplanOperator guifg=#FF6B6B ctermfg=203
highlight folplanDelimiter guifg=#A6ADFF ctermfg=147
highlight folplanComment guifg=#80D995 gui=italic ctermfg=114 cterm=italic

let b:current_syntax = 'folplan'
