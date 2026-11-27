// vars/approve.groovy
def call(String what) {
    timeout(time: 1, unit: 'DAYS') {
        input(message: "Approve ${what}?")
    }
}
