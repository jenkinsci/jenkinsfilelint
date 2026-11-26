pipeline {
    agent any
    stages {
        stage('Outer') {
            stages {
                stage('Inner 1') { steps { echo '1' } }
                stage('Inner 2') { steps { echo '2' } }
            }
        }
        stage('Par') {
            failFast true
            parallel {
                stage('A') {
                    stages {
                        stage('A1') { steps { echo 'a1' } }
                    }
                }
                stage('B') { steps { echo 'b' } }
            }
        }
    }
}
