def helper() {
    pipeline {
        agent any
        stages {
            stage('X') {
                steps { echo 'x' }
                stages {
                    stage('Y') { steps { echo 'y' } }
                }
            }
        }
    }
}

pipeline {
    stages {
        stage('A') {
            steps { echo 'a' }
        }
    }
}
