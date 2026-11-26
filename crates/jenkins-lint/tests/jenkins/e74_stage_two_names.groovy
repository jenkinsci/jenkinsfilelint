pipeline {
    agent any
    stages {
        stage('A', 'B') {
            steps { echo 'x' }
        }
    }
}
