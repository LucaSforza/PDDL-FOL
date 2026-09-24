if exists('b:current_syntax')
  finish
endif

syntax case ignore

syntax keyword folplanSection problem types objects predicates init goal action pre effect
syntax keyword folplanQuantifier forall exists
syntax keyword folplanBoolean true false
syntax keyword folplanLogic and or not implies
syntax match folplanPredicate /\<[A-Za-z][A-Za-z0-9_-]*\ze\s*(/
syntax match folplanOperator /[!&|=]\|->/
syntax match folplanDelimiter /[{}(),;:.]/
syntax match folplanComment /\/\/.*/ contains=@Spell

highlight default link folplanSection Keyword
highlight default link folplanQuantifier Conditional
highlight default link folplanBoolean Boolean
highlight default link folplanLogic Operator
highlight default link folplanPredicate Function
highlight default link folplanOperator Operator
highlight default link folplanDelimiter Delimiter
highlight default link folplanComment Comment

let b:current_syntax = 'folplan'
