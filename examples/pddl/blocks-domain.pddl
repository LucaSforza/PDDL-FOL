(define (domain blocks)
  (:requirements :strips :typing :equality)
  (:types block)
  (:predicates (on ?x - block ?y - block)
               (on-table ?x - block)
               (clear ?x - block))
  (:action unstack
    :parameters (?x - block ?y - block)
    :precondition (and (on ?x ?y) (clear ?x))
    :effect (and (on-table ?x) (clear ?y) (not (on ?x ?y))))
  (:action stack
    :parameters (?x - block ?y - block)
    :precondition (and (on-table ?x) (clear ?x) (clear ?y) (not (= ?x ?y)))
    :effect (and (on ?x ?y) (not (on-table ?x)) (not (clear ?y)))))
