(define (domain logistics)
  (:requirements :strips :typing)
  (:types location package truck)
  (:predicates (at-package ?p - package ?l - location)
               (at-truck ?t - truck ?l - location)
               (road ?from - location ?to - location)
               (in ?p - package ?t - truck))
  (:action drive
    :parameters (?t - truck ?from - location ?to - location)
    :precondition (and (at-truck ?t ?from) (road ?from ?to))
    :effect (and (at-truck ?t ?to) (not (at-truck ?t ?from))))
  (:action load
    :parameters (?p - package ?t - truck ?l - location)
    :precondition (and (at-package ?p ?l) (at-truck ?t ?l))
    :effect (and (in ?p ?t) (not (at-package ?p ?l))))
  (:action unload
    :parameters (?p - package ?t - truck ?l - location)
    :precondition (and (in ?p ?t) (at-truck ?t ?l))
    :effect (and (at-package ?p ?l) (not (in ?p ?t)))))
