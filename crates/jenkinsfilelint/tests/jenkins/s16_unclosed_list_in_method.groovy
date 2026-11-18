def helper() {
    def a = [1, 2
    return a
}
pipeline {
    agent any
    stages { stage('S') { steps { echo 'x' } } }
}
