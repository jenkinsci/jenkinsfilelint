pipeline {
    agent any
    stages {
        stage('Same') { steps { echo '1' } }
        stage('Same') { steps { echo '2' } }
    }
}
