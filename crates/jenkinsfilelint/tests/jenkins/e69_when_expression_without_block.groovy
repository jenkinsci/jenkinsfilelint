pipeline {
    agent any
    stages {
        stage('S') {
            when {
                expression 'x'
            }
            steps { echo 'x' }
        }
    }
}
