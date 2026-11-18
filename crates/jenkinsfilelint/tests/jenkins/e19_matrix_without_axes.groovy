pipeline {
    agent any
    stages {
        stage('S') {
            matrix {
                stages {
                    stage('M') { steps { echo 'x' } }
                }
            }
        }
    }
}
