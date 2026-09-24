(define (domain impossible)
  (:requirements :strips :typing)
  (:types place)
  (:predicates (at ?p - place) (road ?from - place ?to - place))
  (:action move
    :parameters (?from - place ?to - place)
    :precondition (and (at ?from) (road ?from ?to))
    :effect (and (at ?to) (not (at ?from)))))
