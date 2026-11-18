pipeline {
    agent any
    stages {
        stage('S') {
            stages {
                failFast true
                stage('A') { steps { echo 'a' } }
            }
        }
    }
}
