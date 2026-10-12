//! A small built-in list of very frequent German and English words. Used as a
//! false-alarm guard: such words are never suggested as a "correct spelling" and
//! never learned from an edit. Deliberately short - it only has to cover words
//! that sound like names or product terms by accident.

const WORDS: &str = "
aber alle allem allen aller alles als also am an andere anderen auch auf aus bei beim bin bis bist da dabei dafür dann das dass daß dein deine dem den der des die dies diese diesem diesen dieser dieses doch dort du durch ein eine einem einen einer eines einmal er es etwas euch euer für gegen gibt habe haben hat hatte hier hin hinter ich ihr ihre ihm ihn immer in ist ja jede jeder jetzt kann kein keine können könnte machen macht mal man mehr mein meine mich mir mit muss müssen nach nein nicht noch nun nur ob oder ohne sehr sein seine sich sie sind so sollen soll über um und uns unser unter vom von vor war waren was weil weiter wenn wer werden wie wieder will wir wird wirklich wo wollen wurde zu zum zur zwei drei vier fünf sechs sieben acht neun zehn
heute morgen gestern montag dienstag mittwoch donnerstag freitag samstag sonntag termin treffen gespräch anruf nachricht mail rechnung angebot kunde kunden projekt woche monat jahr tag tage uhr zeit stunde minute
januar februar märz april mai juni juli august september oktober november dezember
gut gute guten besser beste neue neuen neuer groß große kleine klein alt alte lang kurz ganz viel viele wenig schon gerne bitte danke hallo tschüss vielleicht genau richtig falsch
haus hand hund katze wasser brot tisch stuhl auto bahn stadt land strasse straße schule arbeit arbeiten leben gehen gehe geht kommen komme kommt sagen sage sagt sehen sehe sieht mache gebe geben nehmen nehme denke denken wissen weiß glaube finden finde brauche brauchen schicken schicke senden
the and for are but not you your with this that have from they their there would could should about then than them these those here were well will what when where which while into just like make made want some time know take come back good much also been over only more most very even such many does done same sure okay yeah hello please thanks thank message email text call meeting today tomorrow yesterday monday tuesday wednesday thursday friday saturday sunday morning evening night week month year people thing things work working need needs send sending check think going gonna right left home house phone number name first last next new old big small
";

/// True if `word` (case-insensitive) is a very frequent German/English word.
pub fn is_common_word(word: &str) -> bool {
    let lc = word.trim().to_lowercase();
    if lc.is_empty() {
        return false;
    }
    WORDS.split_whitespace().any(|w| w == lc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_common_words() {
        assert!(is_common_word("Heute"));
        assert!(is_common_word("Termin"));
        assert!(is_common_word("there"));
        assert!(!is_common_word("HeinsApp"));
        assert!(!is_common_word("Teijin"));
    }
}
